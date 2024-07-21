use audiopus::{
    coder::{Decoder as AudiopusDecoder, GenericCtl}, Channels, Error as OpusError, ErrorCode, SampleRate
};
use symphonia_core::{
    audio::{AsAudioBufferRef, AudioBuffer, AudioBufferRef, Layout, Signal, SignalSpec},
    codecs::{
        CodecDescriptor,
        CodecParameters,
        Decoder,
        DecoderOptions,
        FinalizeResult,
        CODEC_TYPE_OPUS,
    },
    errors::{decode_error, Result as SymphResult},
    formats::Packet,
};

/// Opus decoder for symphonia, based on libopus v1.3 (via [`audiopus`]).
pub struct OpusDecoder {
    inner: AudiopusDecoder,
    params: CodecParameters,
    buf: AudioBuffer<f32>,
    rawbuf: Vec<f32>,
    sample_rate: u32,
}

pub const AUDIO_FRAME_RATE: usize = 50;

/// # SAFETY
/// The underlying Opus decoder (currently) requires only a `&self` parameter
/// to decode given packets, which is likely a mistaken decision.
///
/// This struct makes stronger assumptions and only touches FFI decoder state with a
/// `&mut self`, preventing data races via `&OpusDecoder` as required by `impl Sync`.
/// No access to other internal state relies on unsafety or crosses FFI.
unsafe impl Sync for OpusDecoder {}

impl OpusDecoder {
    fn decode_inner(&mut self, packet: &Packet) -> SymphResult<()> {
        let s_ct = loop {
            let pkt = if packet.buf().is_empty() {
                None
            } else if let Ok(checked_pkt) = packet.buf().try_into() {
                Some(checked_pkt)
            } else {
                return decode_error("Opus packet was too large (greater than i32::MAX bytes).");
            };
            let out_space = (&mut self.rawbuf[..]).try_into().expect("The following logic expands this buffer safely below i32::MAX, and we throw our own error.");

            match self.inner.decode_float(pkt, out_space, false) {
                Ok(v) => break v,
                Err(OpusError::Opus(ErrorCode::BufferTooSmall)) => {
                    // double the buffer size
                    // correct behav would be to mirror the decoder logic in the udp_rx set.
                    let new_size = (self.rawbuf.len() * 2).min(std::i32::MAX as usize);
                    if new_size == self.rawbuf.len() {
                        return decode_error("Opus frame too big: cannot expand opus frame decode buffer any further.");
                    }

                    self.rawbuf.resize(new_size, 0.0);
                    self.buf = AudioBuffer::new(
                        self.rawbuf.len() as u64 / 2,
                        SignalSpec::new_with_layout(self.sample_rate, Layout::Stereo),
                    );
                },
                Err(e) => {
                    println!("Opus decode error: {:?}", e);
                    return decode_error("Opus decode error: see 'tracing' logs.");
                },
            }
        };

        self.buf.clear();
        self.buf.render_reserved(Some(s_ct));

        // Forcibly assuming stereo, for now.
        for ch in 0..2 {
            let iter = self.rawbuf.chunks_exact(2).map(|chunk| chunk[ch]);
            for (tgt, src) in self.buf.chan_mut(ch).iter_mut().zip(iter) {
                *tgt = src;
            }
        }

        Ok(())
    }
}

impl Decoder for OpusDecoder {
    fn try_new(params: &CodecParameters, _options: &DecoderOptions) -> SymphResult<Self> {
        let (sample_rate, sample_rate_raw) = match params.sample_rate {
            Some(48000) => {
                (SampleRate::Hz48000, 48000)
            },
            Some(24000) => {
                (SampleRate::Hz24000, 24000)
            },
            Some(16000) => {
                (SampleRate::Hz16000, 16000)
            },
            Some(12000) => {
                (SampleRate::Hz12000, 12000)
            },
            Some(8000) => {
                (SampleRate::Hz8000, 8000)
            },
            e => {
                println!("No sample rate provided {:?}", e);
                panic!()
            },
        };
        let inner = AudiopusDecoder::new(sample_rate, Channels::Stereo).unwrap();

        let mut params = params.clone();
        params.with_sample_rate(sample_rate_raw);

        let mono_frame_size = sample_rate_raw as usize / AUDIO_FRAME_RATE;
        let stereo_frame_size = mono_frame_size * 2;

        Ok(Self {
            inner,
            params,
            buf: AudioBuffer::new(
                mono_frame_size as  u64,
                SignalSpec::new_with_layout(sample_rate_raw, Layout::Stereo),
            ),
            rawbuf: vec![0.0f32; stereo_frame_size],
            sample_rate: sample_rate_raw
        })
    }

    fn supported_codecs() -> &'static [CodecDescriptor] {
        &[symphonia_core::support_codec!(
            CODEC_TYPE_OPUS,
            "opus",
            "libopus (1.3+, audiopus)"
        )]
    }

    fn codec_params(&self) -> &CodecParameters {
        &self.params
    }

    fn decode(&mut self, packet: &Packet) -> SymphResult<AudioBufferRef<'_>> {
        if let Err(e) = self.decode_inner(packet) {
            self.buf.clear();
            Err(e)
        } else {
            Ok(self.buf.as_audio_buffer_ref())
        }
    }

    fn reset(&mut self) {
        _ = self.inner.reset_state();
    }

    fn finalize(&mut self) -> FinalizeResult {
        FinalizeResult::default()
    }

    fn last_decoded(&self) -> AudioBufferRef<'_> {
        self.buf.as_audio_buffer_ref()
    }
}

//impl Sound for OpusDecoder {
//    fn channel_count(&self) -> u16 {
//        let channels = self.params.channels.unwrap();
//        channels.count() as u16
//    }
//
//    fn sample_rate(&self) -> u32 {
//        self.params.sample_rate.unwrap()
//    }
//
//    fn next_sample(&mut self) -> Result<awedio::NextSample, awedio::Error> {
//        if self.next_channel_idx >= self.channels.count().try_into().unwrap() {
//            self.next_channel_idx = 0;
//            self.next_sample_idx += 1;
//        }
//        let mut buf_ref = self.decoder.last_decoded();
//        if self.next_sample_idx >= buf_ref.frames() {
//            match self.decode_next_packet() {
//                Ok(true) => return Ok(NextSample::MetadataChanged),
//                Ok(false) => (),
//                Err(Error::IoError(err))
//                    if err.kind() == std::io::ErrorKind::UnexpectedEof
//                        && err.to_string() == "end of stream" =>
//                {
//                    // According to Symphonia this is the only way to detect an end of stream
//                    return Ok(NextSample::Finished);
//                }
//                // TODO: Handle errors better when awedio allows returning errors.
//                Err(e) => return Err(e.into()),
//            };
//            buf_ref = self.decoder.last_decoded();
//        }
//        let sample = extract_sample_from_ref(&buf_ref, self.next_channel_idx, self.next_sample_idx);
//        self.next_channel_idx += 1;
//        Ok(NextSample::Sample(sample))
//    }
//
//    fn on_start_of_batch(&mut self) {
//        todo!()
//    }
//}
