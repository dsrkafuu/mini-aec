#pragma once

static KSDATAFORMAT_WAVEFORMATEXTENSIBLE MiniAecPinSupportedDeviceFormats[] = {
    {{sizeof(KSDATAFORMAT_WAVEFORMATEXTENSIBLE), 0, 0, 0,
      STATICGUIDOF(KSDATAFORMAT_TYPE_AUDIO),
      STATICGUIDOF(KSDATAFORMAT_SUBTYPE_PCM),
      STATICGUIDOF(KSDATAFORMAT_SPECIFIER_WAVEFORMATEX)},
     {{WAVE_FORMAT_EXTENSIBLE, MINIAEC_CHANNELS, MINIAEC_SAMPLE_RATE,
       MINIAEC_SAMPLE_RATE *MINIAEC_FRAME_BYTES / MINIAEC_FRAME_SAMPLES,
       MINIAEC_FRAME_BYTES / MINIAEC_FRAME_SAMPLES, MINIAEC_BITS_PER_SAMPLE,
       sizeof(WAVEFORMATEXTENSIBLE) - sizeof(WAVEFORMATEX)},
      MINIAEC_BITS_PER_SAMPLE,
      KSAUDIO_SPEAKER_MONO,
      STATICGUIDOF(KSDATAFORMAT_SUBTYPE_PCM)}}};

static MODE_AND_DEFAULT_FORMAT MiniAecPinSupportedDeviceModes[] = {
    {
        STATIC_AUDIO_SIGNALPROCESSINGMODE_RAW,
        &MiniAecPinSupportedDeviceFormats[0].DataFormat,
    },
    {
        STATIC_AUDIO_SIGNALPROCESSINGMODE_DEFAULT,
        &MiniAecPinSupportedDeviceFormats[0].DataFormat,
    },
    {
        STATIC_AUDIO_SIGNALPROCESSINGMODE_COMMUNICATIONS,
        &MiniAecPinSupportedDeviceFormats[0].DataFormat,
    },
};

static PIN_DEVICE_FORMATS_AND_MODES MiniAecPinDeviceFormatsAndModes[] = {
    {BridgePin, NULL, 0, NULL, 0},
    {SystemCapturePin, MiniAecPinSupportedDeviceFormats,
     SIZEOF_ARRAY(MiniAecPinSupportedDeviceFormats),
     MiniAecPinSupportedDeviceModes,
     SIZEOF_ARRAY(MiniAecPinSupportedDeviceModes)}};
