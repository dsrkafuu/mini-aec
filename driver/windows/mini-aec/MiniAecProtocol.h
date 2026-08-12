#pragma once

#include <ntddk.h>

#define MINIAEC_PROTOCOL_MAGIC 0x4345414dUL
#define MINIAEC_PROTOCOL_VERSION 1U
#define MINIAEC_DIAGNOSTICS_SCHEMA_VERSION 2U

#define MINIAEC_SAMPLE_RATE 48000UL
#define MINIAEC_CHANNELS 1U
#define MINIAEC_BITS_PER_SAMPLE 16U
#define MINIAEC_FRAME_SAMPLES 480U
#define MINIAEC_FRAME_BYTES 960U
#define MINIAEC_RING_CAPACITY 10U

#define MINIAEC_DEVICE_PATH L"\\Device\\MiniAECTransport"
#define MINIAEC_DOS_DEVICE_PATH L"\\DosDevices\\MiniAECTransport"

#define IOCTL_MINIAEC_OPEN_SESSION                                             \
  CTL_CODE(FILE_DEVICE_UNKNOWN, 0x800, METHOD_BUFFERED, FILE_WRITE_DATA)
#define IOCTL_MINIAEC_WRITE_FRAME                                              \
  CTL_CODE(FILE_DEVICE_UNKNOWN, 0x801, METHOD_BUFFERED, FILE_WRITE_DATA)
#define IOCTL_MINIAEC_GET_DIAGNOSTICS                                          \
  CTL_CODE(FILE_DEVICE_UNKNOWN, 0x802, METHOD_BUFFERED, FILE_READ_DATA)
#define IOCTL_MINIAEC_CLOSE_SESSION                                            \
  CTL_CODE(FILE_DEVICE_UNKNOWN, 0x803, METHOD_BUFFERED, FILE_WRITE_DATA)

typedef enum _MINIAEC_SESSION_STATE {
  MiniAecSessionClosed = 0,
  MiniAecSessionOpen = 1,
} MINIAEC_SESSION_STATE;

#pragma pack(push, 1)

typedef struct _MINIAEC_OPEN_SESSION_REQUEST {
  ULONG Magic;
  USHORT ProtocolVersion;
  USHORT HeaderSize;
  ULONG TotalSize;
  UCHAR SessionId[16];
  ULONG SampleRate;
  USHORT Channels;
  USHORT BitsPerSample;
  USHORT FrameSamples;
  USHORT Reserved;
} MINIAEC_OPEN_SESSION_REQUEST, *PMINIAEC_OPEN_SESSION_REQUEST;

typedef struct _MINIAEC_WRITE_FRAME_REQUEST {
  ULONG Magic;
  USHORT ProtocolVersion;
  USHORT HeaderSize;
  ULONG TotalSize;
  UCHAR SessionId[16];
  ULONGLONG Sequence;
  ULONG PayloadSize;
  ULONG Reserved;
  UCHAR Pcm[MINIAEC_FRAME_BYTES];
} MINIAEC_WRITE_FRAME_REQUEST, *PMINIAEC_WRITE_FRAME_REQUEST;

typedef struct _MINIAEC_CLOSE_SESSION_REQUEST {
  ULONG Magic;
  USHORT ProtocolVersion;
  USHORT HeaderSize;
  ULONG TotalSize;
  UCHAR SessionId[16];
} MINIAEC_CLOSE_SESSION_REQUEST, *PMINIAEC_CLOSE_SESSION_REQUEST;

typedef struct _MINIAEC_DIAGNOSTICS {
  ULONG Magic;
  USHORT ProtocolVersion;
  USHORT SchemaVersion;
  ULONG TotalSize;
  ULONG SessionState;
  ULONG CurrentDepth;
  ULONG HighWaterMark;
  UCHAR SessionId[16];
  ULONGLONG LastAcceptedSequence;
  ULONG HasLastAcceptedSequence;
  ULONG Reserved;
  ULONGLONG SessionOpens;
  ULONGLONG SessionCloses;
  ULONGLONG SessionResets;
  ULONGLONG AcceptedFrames;
  ULONGLONG RejectedWrites;
  ULONGLONG Underruns;
  ULONGLONG Overflows;
  ULONGLONG DiscardedFrames;
  ULONGLONG DriverRestarts;
} MINIAEC_DIAGNOSTICS, *PMINIAEC_DIAGNOSTICS;

#pragma pack(pop)

C_ASSERT(sizeof(MINIAEC_OPEN_SESSION_REQUEST) == 40);
C_ASSERT(sizeof(MINIAEC_WRITE_FRAME_REQUEST) == 1004);
C_ASSERT(sizeof(MINIAEC_CLOSE_SESSION_REQUEST) == 28);
C_ASSERT(sizeof(MINIAEC_DIAGNOSTICS) == 128);
