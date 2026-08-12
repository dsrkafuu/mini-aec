#pragma once

// Protected DACL: SYSTEM and Builtin Administrators retain full control while a local interactive
// user receives only the read/write rights required by the versioned producer IOCTLs.
#define MINIAEC_TRANSPORT_SDDL L"D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;IU)"
