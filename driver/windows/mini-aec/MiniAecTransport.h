#pragma once

#include <ntddk.h>

#include "MiniAecProtocol.h"

EXTERN_C_START

_IRQL_requires_max_(PASSIVE_LEVEL) NTSTATUS
    MiniAecTransportInitialize(_In_ PDRIVER_OBJECT DriverObject);

_IRQL_requires_max_(PASSIVE_LEVEL) VOID
    MiniAecTransportShutdown(_In_ PDRIVER_OBJECT DriverObject);

_IRQL_requires_max_(DISPATCH_LEVEL) ULONG
    MiniAecTransportGetCapturePeakMagnitude(VOID);

_IRQL_requires_max_(DISPATCH_LEVEL) VOID
    MiniAecTransportReadCapture(_Out_writes_bytes_(ByteCount) PUCHAR Buffer,
                                _In_ ULONG ByteCount);

EXTERN_C_END
