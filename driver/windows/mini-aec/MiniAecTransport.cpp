#include <ntddk.h>
#include <wdmsec.h>

#include "MiniAecSecurity.h"
#include "MiniAecTransport.h"

namespace
{
struct MiniAecRingFrame
{
    ULONGLONG Sequence;
    UCHAR Pcm[MINIAEC_FRAME_BYTES];
};

struct MiniAecTransportState
{
    KSPIN_LOCK Lock;
    PDEVICE_OBJECT ControlDevice;
    PFILE_OBJECT OwnerFile;
    BOOLEAN DispatchInstalled;
    BOOLEAN SymbolicLinkCreated;
    BOOLEAN SessionActive;
    UCHAR SessionId[16];
    BOOLEAN HasLastAcceptedSequence;
    ULONGLONG LastAcceptedSequence;
    ULONGLONG NextSequence;
    MiniAecRingFrame Ring[MINIAEC_RING_CAPACITY];
    ULONG ReadIndex;
    ULONG WriteIndex;
    ULONG RingCount;
    UCHAR CaptureFrame[MINIAEC_FRAME_BYTES];
    ULONG CaptureOffset;
    ULONG HighWaterMark;
    ULONGLONG SessionOpens;
    ULONGLONG SessionCloses;
    ULONGLONG SessionResets;
    ULONGLONG AcceptedFrames;
    ULONGLONG RejectedWrites;
    ULONGLONG Underruns;
    ULONGLONG Overflows;
    ULONGLONG DiscardedFrames;
    ULONGLONG DriverRestarts;
    PDRIVER_DISPATCH OriginalCreate;
    PDRIVER_DISPATCH OriginalCleanup;
    PDRIVER_DISPATCH OriginalClose;
    PDRIVER_DISPATCH OriginalDeviceControl;
};

MiniAecTransportState g_State = {};

const GUID MiniAecTransportClassGuid =
{ 0x8f6f2d18, 0xf2d5, 0x4f47, { 0x9d, 0x73, 0xb2, 0x44, 0x36, 0x70, 0x6f, 0x87 } };

NTSTATUS CompleteIrp(_In_ PIRP Irp, _In_ NTSTATUS Status, _In_ ULONG_PTR Information = 0)
{
    Irp->IoStatus.Status = Status;
    Irp->IoStatus.Information = Information;
    IoCompleteRequest(Irp, IO_NO_INCREMENT);
    return Status;
}

NTSTATUS ForwardIrp(
    _In_ PDEVICE_OBJECT DeviceObject,
    _In_ PIRP Irp,
    _In_opt_ PDRIVER_DISPATCH OriginalDispatch
    )
{
    if (OriginalDispatch == nullptr)
    {
        return CompleteIrp(Irp, STATUS_INVALID_DEVICE_REQUEST);
    }

    return OriginalDispatch(DeviceObject, Irp);
}

BOOLEAN SessionIdIsZero(_In_reads_(16) const UCHAR* SessionId)
{
    UCHAR combined = 0;
    for (ULONG index = 0; index < 16; ++index)
    {
        combined |= SessionId[index];
    }
    return combined == 0;
}

BOOLEAN SessionIdEquals(
    _In_reads_(16) const UCHAR* Left,
    _In_reads_(16) const UCHAR* Right
    )
{
    return RtlCompareMemory(Left, Right, 16) == 16;
}

VOID ResetAudioLocked()
{
    RtlZeroMemory(g_State.Ring, sizeof(g_State.Ring));
    g_State.ReadIndex = 0;
    g_State.WriteIndex = 0;
    g_State.RingCount = 0;
    RtlZeroMemory(g_State.CaptureFrame, sizeof(g_State.CaptureFrame));
    g_State.CaptureOffset = MINIAEC_FRAME_BYTES;
}

VOID CloseSessionLocked()
{
    if (g_State.SessionActive)
    {
        ++g_State.SessionCloses;
    }
    g_State.SessionActive = FALSE;
    RtlZeroMemory(g_State.SessionId, sizeof(g_State.SessionId));
    g_State.HasLastAcceptedSequence = FALSE;
    g_State.LastAcceptedSequence = 0;
    g_State.NextSequence = 0;
    ResetAudioLocked();
}

BOOLEAN IsOwnerLocked(_In_opt_ PFILE_OBJECT FileObject)
{
    return FileObject != nullptr && g_State.OwnerFile == FileObject;
}

VOID IncrementRejectedWrite()
{
    KIRQL oldIrql;
    KeAcquireSpinLock(&g_State.Lock, &oldIrql);
    ++g_State.RejectedWrites;
    KeReleaseSpinLock(&g_State.Lock, oldIrql);
}

NTSTATUS ValidateCommonHeader(
    _In_ ULONG Magic,
    _In_ USHORT ProtocolVersion,
    _In_ USHORT HeaderSize,
    _In_ ULONG ExpectedHeaderSize,
    _In_ ULONG TotalSize,
    _In_ ULONG ExpectedTotalSize
    )
{
    if (Magic != MINIAEC_PROTOCOL_MAGIC)
    {
        return STATUS_INVALID_PARAMETER;
    }
    if (ProtocolVersion != MINIAEC_PROTOCOL_VERSION)
    {
        return STATUS_REVISION_MISMATCH;
    }
    if (HeaderSize != ExpectedHeaderSize || TotalSize != ExpectedTotalSize)
    {
        return STATUS_INVALID_BUFFER_SIZE;
    }
    return STATUS_SUCCESS;
}

NTSTATUS HandleOpenSession(
    _In_ PFILE_OBJECT FileObject,
    _In_reads_bytes_(InputLength) const VOID* InputBuffer,
    _In_ ULONG InputLength
    )
{
    if (InputBuffer == nullptr || InputLength != sizeof(MINIAEC_OPEN_SESSION_REQUEST))
    {
        return STATUS_INVALID_BUFFER_SIZE;
    }

    const auto request = static_cast<const MINIAEC_OPEN_SESSION_REQUEST*>(InputBuffer);
    NTSTATUS status = ValidateCommonHeader(
        request->Magic,
        request->ProtocolVersion,
        request->HeaderSize,
        sizeof(MINIAEC_OPEN_SESSION_REQUEST),
        request->TotalSize,
        sizeof(MINIAEC_OPEN_SESSION_REQUEST));
    if (!NT_SUCCESS(status))
    {
        return status;
    }
    if (SessionIdIsZero(request->SessionId) ||
        request->SampleRate != MINIAEC_SAMPLE_RATE ||
        request->Channels != MINIAEC_CHANNELS ||
        request->BitsPerSample != MINIAEC_BITS_PER_SAMPLE ||
        request->FrameSamples != MINIAEC_FRAME_SAMPLES ||
        request->Reserved != 0)
    {
        return STATUS_INVALID_PARAMETER;
    }

    KIRQL oldIrql;
    KeAcquireSpinLock(&g_State.Lock, &oldIrql);
    if (!IsOwnerLocked(FileObject))
    {
        status = STATUS_ACCESS_DENIED;
    }
    else if (g_State.SessionActive)
    {
        status = STATUS_INVALID_DEVICE_STATE;
    }
    else
    {
        if (g_State.SessionOpens != 0)
        {
            ++g_State.SessionResets;
        }
        ResetAudioLocked();
        RtlCopyMemory(g_State.SessionId, request->SessionId, sizeof(g_State.SessionId));
        g_State.HasLastAcceptedSequence = FALSE;
        g_State.LastAcceptedSequence = 0;
        g_State.NextSequence = 0;
        g_State.SessionActive = TRUE;
        ++g_State.SessionOpens;
        status = STATUS_SUCCESS;
    }
    KeReleaseSpinLock(&g_State.Lock, oldIrql);
    return status;
}

NTSTATUS HandleWriteFrame(
    _In_ PFILE_OBJECT FileObject,
    _In_reads_bytes_(InputLength) const VOID* InputBuffer,
    _In_ ULONG InputLength
    )
{
    if (InputBuffer == nullptr || InputLength != sizeof(MINIAEC_WRITE_FRAME_REQUEST))
    {
        IncrementRejectedWrite();
        return STATUS_INVALID_BUFFER_SIZE;
    }

    const auto request = static_cast<const MINIAEC_WRITE_FRAME_REQUEST*>(InputBuffer);
    NTSTATUS status = ValidateCommonHeader(
        request->Magic,
        request->ProtocolVersion,
        request->HeaderSize,
        FIELD_OFFSET(MINIAEC_WRITE_FRAME_REQUEST, Pcm),
        request->TotalSize,
        sizeof(MINIAEC_WRITE_FRAME_REQUEST));
    if (!NT_SUCCESS(status) || request->PayloadSize != MINIAEC_FRAME_BYTES || request->Reserved != 0)
    {
        IncrementRejectedWrite();
        return NT_SUCCESS(status) ? STATUS_INVALID_BUFFER_SIZE : status;
    }

    KIRQL oldIrql;
    KeAcquireSpinLock(&g_State.Lock, &oldIrql);
    if (!IsOwnerLocked(FileObject))
    {
        status = STATUS_ACCESS_DENIED;
    }
    else if (!g_State.SessionActive)
    {
        status = STATUS_INVALID_DEVICE_STATE;
    }
    else if (!SessionIdEquals(g_State.SessionId, request->SessionId))
    {
        status = STATUS_INVALID_DEVICE_STATE;
    }
    else if (request->Sequence != g_State.NextSequence ||
             (g_State.HasLastAcceptedSequence && g_State.LastAcceptedSequence == MAXULONGLONG))
    {
        status = STATUS_INVALID_PARAMETER;
    }
    else
    {
        if (g_State.RingCount == MINIAEC_RING_CAPACITY)
        {
            g_State.ReadIndex = (g_State.ReadIndex + 1) % MINIAEC_RING_CAPACITY;
            --g_State.RingCount;
            ++g_State.Overflows;
            ++g_State.DiscardedFrames;
        }

        MiniAecRingFrame* frame = &g_State.Ring[g_State.WriteIndex];
        frame->Sequence = request->Sequence;
        RtlCopyMemory(frame->Pcm, request->Pcm, MINIAEC_FRAME_BYTES);
        g_State.WriteIndex = (g_State.WriteIndex + 1) % MINIAEC_RING_CAPACITY;
        ++g_State.RingCount;
        if (g_State.RingCount > g_State.HighWaterMark)
        {
            g_State.HighWaterMark = g_State.RingCount;
        }
        g_State.HasLastAcceptedSequence = TRUE;
        g_State.LastAcceptedSequence = request->Sequence;
        g_State.NextSequence = request->Sequence + 1;
        ++g_State.AcceptedFrames;
        status = STATUS_SUCCESS;
    }

    if (!NT_SUCCESS(status))
    {
        ++g_State.RejectedWrites;
    }
    KeReleaseSpinLock(&g_State.Lock, oldIrql);
    return status;
}

NTSTATUS HandleGetDiagnostics(
    _In_ PFILE_OBJECT FileObject,
    _Out_writes_bytes_(OutputLength) VOID* OutputBuffer,
    _In_ ULONG OutputLength,
    _Out_ PULONG_PTR Information
    )
{
    if (OutputBuffer == nullptr || OutputLength < sizeof(MINIAEC_DIAGNOSTICS))
    {
        return STATUS_BUFFER_TOO_SMALL;
    }

    auto diagnostics = static_cast<MINIAEC_DIAGNOSTICS*>(OutputBuffer);
    RtlZeroMemory(diagnostics, sizeof(*diagnostics));

    KIRQL oldIrql;
    KeAcquireSpinLock(&g_State.Lock, &oldIrql);
    if (!IsOwnerLocked(FileObject))
    {
        KeReleaseSpinLock(&g_State.Lock, oldIrql);
        return STATUS_ACCESS_DENIED;
    }

    diagnostics->Magic = MINIAEC_PROTOCOL_MAGIC;
    diagnostics->ProtocolVersion = MINIAEC_PROTOCOL_VERSION;
    diagnostics->SchemaVersion = MINIAEC_DIAGNOSTICS_SCHEMA_VERSION;
    diagnostics->TotalSize = sizeof(*diagnostics);
    diagnostics->SessionState = g_State.SessionActive ? MiniAecSessionOpen : MiniAecSessionClosed;
    diagnostics->CurrentDepth = g_State.RingCount;
    diagnostics->HighWaterMark = g_State.HighWaterMark;
    RtlCopyMemory(diagnostics->SessionId, g_State.SessionId, sizeof(diagnostics->SessionId));
    diagnostics->LastAcceptedSequence = g_State.LastAcceptedSequence;
    diagnostics->HasLastAcceptedSequence = g_State.HasLastAcceptedSequence;
    diagnostics->SessionOpens = g_State.SessionOpens;
    diagnostics->SessionCloses = g_State.SessionCloses;
    diagnostics->SessionResets = g_State.SessionResets;
    diagnostics->AcceptedFrames = g_State.AcceptedFrames;
    diagnostics->RejectedWrites = g_State.RejectedWrites;
    diagnostics->Underruns = g_State.Underruns;
    diagnostics->Overflows = g_State.Overflows;
    diagnostics->DiscardedFrames = g_State.DiscardedFrames;
    diagnostics->DriverRestarts = g_State.DriverRestarts;
    KeReleaseSpinLock(&g_State.Lock, oldIrql);

    *Information = sizeof(*diagnostics);
    return STATUS_SUCCESS;
}

NTSTATUS HandleCloseSession(
    _In_ PFILE_OBJECT FileObject,
    _In_reads_bytes_(InputLength) const VOID* InputBuffer,
    _In_ ULONG InputLength
    )
{
    if (InputBuffer == nullptr || InputLength != sizeof(MINIAEC_CLOSE_SESSION_REQUEST))
    {
        return STATUS_INVALID_BUFFER_SIZE;
    }

    const auto request = static_cast<const MINIAEC_CLOSE_SESSION_REQUEST*>(InputBuffer);
    NTSTATUS status = ValidateCommonHeader(
        request->Magic,
        request->ProtocolVersion,
        request->HeaderSize,
        sizeof(MINIAEC_CLOSE_SESSION_REQUEST),
        request->TotalSize,
        sizeof(MINIAEC_CLOSE_SESSION_REQUEST));
    if (!NT_SUCCESS(status))
    {
        return status;
    }

    KIRQL oldIrql;
    KeAcquireSpinLock(&g_State.Lock, &oldIrql);
    if (!IsOwnerLocked(FileObject))
    {
        status = STATUS_ACCESS_DENIED;
    }
    else if (!g_State.SessionActive)
    {
        status = STATUS_SUCCESS;
    }
    else if (!SessionIdEquals(g_State.SessionId, request->SessionId))
    {
        status = STATUS_INVALID_DEVICE_STATE;
    }
    else
    {
        CloseSessionLocked();
        status = STATUS_SUCCESS;
    }
    KeReleaseSpinLock(&g_State.Lock, oldIrql);
    return status;
}

_Dispatch_type_(IRP_MJ_CREATE)
NTSTATUS MiniAecDispatchCreate(_In_ PDEVICE_OBJECT DeviceObject, _In_ PIRP Irp)
{
    if (DeviceObject != g_State.ControlDevice)
    {
        return ForwardIrp(DeviceObject, Irp, g_State.OriginalCreate);
    }

    PIO_STACK_LOCATION stack = IoGetCurrentIrpStackLocation(Irp);
    NTSTATUS status;
    KIRQL oldIrql;
    KeAcquireSpinLock(&g_State.Lock, &oldIrql);
    if (stack->FileObject == nullptr)
    {
        status = STATUS_INVALID_PARAMETER;
    }
    else if (g_State.OwnerFile != nullptr)
    {
        status = STATUS_DEVICE_BUSY;
    }
    else
    {
        g_State.OwnerFile = stack->FileObject;
        status = STATUS_SUCCESS;
    }
    KeReleaseSpinLock(&g_State.Lock, oldIrql);
    return CompleteIrp(Irp, status);
}

NTSTATUS ReleaseOwner(_In_ PFILE_OBJECT FileObject)
{
    KIRQL oldIrql;
    KeAcquireSpinLock(&g_State.Lock, &oldIrql);
    if (IsOwnerLocked(FileObject))
    {
        CloseSessionLocked();
        g_State.OwnerFile = nullptr;
    }
    KeReleaseSpinLock(&g_State.Lock, oldIrql);
    return STATUS_SUCCESS;
}

_Dispatch_type_(IRP_MJ_CLEANUP)
NTSTATUS MiniAecDispatchCleanup(_In_ PDEVICE_OBJECT DeviceObject, _In_ PIRP Irp)
{
    if (DeviceObject != g_State.ControlDevice)
    {
        return ForwardIrp(DeviceObject, Irp, g_State.OriginalCleanup);
    }

    PIO_STACK_LOCATION stack = IoGetCurrentIrpStackLocation(Irp);
    NTSTATUS status = ReleaseOwner(stack->FileObject);
    return CompleteIrp(Irp, status);
}

_Dispatch_type_(IRP_MJ_CLOSE)
NTSTATUS MiniAecDispatchClose(_In_ PDEVICE_OBJECT DeviceObject, _In_ PIRP Irp)
{
    if (DeviceObject != g_State.ControlDevice)
    {
        return ForwardIrp(DeviceObject, Irp, g_State.OriginalClose);
    }

    PIO_STACK_LOCATION stack = IoGetCurrentIrpStackLocation(Irp);
    NTSTATUS status = ReleaseOwner(stack->FileObject);
    return CompleteIrp(Irp, status);
}

_Dispatch_type_(IRP_MJ_DEVICE_CONTROL)
NTSTATUS MiniAecDispatchDeviceControl(_In_ PDEVICE_OBJECT DeviceObject, _In_ PIRP Irp)
{
    if (DeviceObject != g_State.ControlDevice)
    {
        return ForwardIrp(DeviceObject, Irp, g_State.OriginalDeviceControl);
    }

    PIO_STACK_LOCATION stack = IoGetCurrentIrpStackLocation(Irp);
    const ULONG code = stack->Parameters.DeviceIoControl.IoControlCode;
    const ULONG inputLength = stack->Parameters.DeviceIoControl.InputBufferLength;
    const ULONG outputLength = stack->Parameters.DeviceIoControl.OutputBufferLength;
    VOID* systemBuffer = Irp->AssociatedIrp.SystemBuffer;
    ULONG_PTR information = 0;
    NTSTATUS status;

    switch (code)
    {
    case IOCTL_MINIAEC_OPEN_SESSION:
        status = HandleOpenSession(stack->FileObject, systemBuffer, inputLength);
        break;
    case IOCTL_MINIAEC_WRITE_FRAME:
        status = HandleWriteFrame(stack->FileObject, systemBuffer, inputLength);
        break;
    case IOCTL_MINIAEC_GET_DIAGNOSTICS:
        status = HandleGetDiagnostics(stack->FileObject, systemBuffer, outputLength, &information);
        break;
    case IOCTL_MINIAEC_CLOSE_SESSION:
        status = HandleCloseSession(stack->FileObject, systemBuffer, inputLength);
        break;
    default:
        status = STATUS_INVALID_DEVICE_REQUEST;
        break;
    }

    return CompleteIrp(Irp, status, information);
}

VOID RestoreDispatch(_In_ PDRIVER_OBJECT DriverObject)
{
    if (!g_State.DispatchInstalled)
    {
        return;
    }
    DriverObject->MajorFunction[IRP_MJ_CREATE] = g_State.OriginalCreate;
    DriverObject->MajorFunction[IRP_MJ_CLEANUP] = g_State.OriginalCleanup;
    DriverObject->MajorFunction[IRP_MJ_CLOSE] = g_State.OriginalClose;
    DriverObject->MajorFunction[IRP_MJ_DEVICE_CONTROL] = g_State.OriginalDeviceControl;
    g_State.DispatchInstalled = FALSE;
}
}

_IRQL_requires_max_(PASSIVE_LEVEL)
NTSTATUS
MiniAecTransportInitialize(
    _In_ PDRIVER_OBJECT DriverObject
    )
{
    PAGED_CODE();

    RtlZeroMemory(&g_State, sizeof(g_State));
    KeInitializeSpinLock(&g_State.Lock);
    g_State.CaptureOffset = MINIAEC_FRAME_BYTES;

    g_State.OriginalCreate = DriverObject->MajorFunction[IRP_MJ_CREATE];
    g_State.OriginalCleanup = DriverObject->MajorFunction[IRP_MJ_CLEANUP];
    g_State.OriginalClose = DriverObject->MajorFunction[IRP_MJ_CLOSE];
    g_State.OriginalDeviceControl = DriverObject->MajorFunction[IRP_MJ_DEVICE_CONTROL];
    DriverObject->MajorFunction[IRP_MJ_CREATE] = MiniAecDispatchCreate;
    DriverObject->MajorFunction[IRP_MJ_CLEANUP] = MiniAecDispatchCleanup;
    DriverObject->MajorFunction[IRP_MJ_CLOSE] = MiniAecDispatchClose;
    DriverObject->MajorFunction[IRP_MJ_DEVICE_CONTROL] = MiniAecDispatchDeviceControl;
    g_State.DispatchInstalled = TRUE;

    UNICODE_STRING deviceName;
    UNICODE_STRING symbolicLink;
    UNICODE_STRING securityDescriptor;
    RtlInitUnicodeString(&deviceName, MINIAEC_DEVICE_PATH);
    RtlInitUnicodeString(&symbolicLink, MINIAEC_DOS_DEVICE_PATH);
    RtlInitUnicodeString(&securityDescriptor, MINIAEC_TRANSPORT_SDDL);

    NTSTATUS status = IoCreateDeviceSecure(
        DriverObject,
        0,
        &deviceName,
        FILE_DEVICE_UNKNOWN,
        FILE_DEVICE_SECURE_OPEN,
        FALSE,
        &securityDescriptor,
        &MiniAecTransportClassGuid,
        &g_State.ControlDevice);
    if (!NT_SUCCESS(status))
    {
        RestoreDispatch(DriverObject);
        return status;
    }

    g_State.ControlDevice->Flags |= DO_BUFFERED_IO;
    status = IoCreateSymbolicLink(&symbolicLink, &deviceName);
    if (!NT_SUCCESS(status))
    {
        IoDeleteDevice(g_State.ControlDevice);
        g_State.ControlDevice = nullptr;
        RestoreDispatch(DriverObject);
        return status;
    }

    g_State.SymbolicLinkCreated = TRUE;
    g_State.ControlDevice->Flags &= ~DO_DEVICE_INITIALIZING;
    return STATUS_SUCCESS;
}

_IRQL_requires_max_(PASSIVE_LEVEL)
VOID
MiniAecTransportShutdown(
    _In_ PDRIVER_OBJECT DriverObject
    )
{
    PAGED_CODE();

    KIRQL oldIrql;
    KeAcquireSpinLock(&g_State.Lock, &oldIrql);
    CloseSessionLocked();
    g_State.OwnerFile = nullptr;
    KeReleaseSpinLock(&g_State.Lock, oldIrql);

    if (g_State.SymbolicLinkCreated)
    {
        UNICODE_STRING symbolicLink;
        RtlInitUnicodeString(&symbolicLink, MINIAEC_DOS_DEVICE_PATH);
        IoDeleteSymbolicLink(&symbolicLink);
        g_State.SymbolicLinkCreated = FALSE;
    }
    if (g_State.ControlDevice != nullptr)
    {
        IoDeleteDevice(g_State.ControlDevice);
        g_State.ControlDevice = nullptr;
    }
    RestoreDispatch(DriverObject);
}

_IRQL_requires_max_(DISPATCH_LEVEL)
VOID
MiniAecTransportReadCapture(
    _Out_writes_bytes_(ByteCount) PUCHAR Buffer,
    _In_ ULONG ByteCount
    )
{
    if (Buffer == nullptr || ByteCount == 0)
    {
        return;
    }

    KIRQL oldIrql;
    KeAcquireSpinLock(&g_State.Lock, &oldIrql);
    ULONG destinationOffset = 0;
    while (destinationOffset < ByteCount)
    {
        if (g_State.CaptureOffset == MINIAEC_FRAME_BYTES)
        {
            if (g_State.RingCount != 0)
            {
                RtlCopyMemory(
                    g_State.CaptureFrame,
                    g_State.Ring[g_State.ReadIndex].Pcm,
                    MINIAEC_FRAME_BYTES);
                g_State.ReadIndex = (g_State.ReadIndex + 1) % MINIAEC_RING_CAPACITY;
                --g_State.RingCount;
            }
            else
            {
                RtlZeroMemory(g_State.CaptureFrame, sizeof(g_State.CaptureFrame));
                if (g_State.SessionActive)
                {
                    ++g_State.Underruns;
                }
            }
            g_State.CaptureOffset = 0;
        }

        const ULONG captureAvailable = MINIAEC_FRAME_BYTES - g_State.CaptureOffset;
        const ULONG destinationAvailable = ByteCount - destinationOffset;
        const ULONG copyLength = min(captureAvailable, destinationAvailable);
        RtlCopyMemory(
            Buffer + destinationOffset,
            g_State.CaptureFrame + g_State.CaptureOffset,
            copyLength);
        g_State.CaptureOffset += copyLength;
        destinationOffset += copyLength;
    }
    KeReleaseSpinLock(&g_State.Lock, oldIrql);
}
