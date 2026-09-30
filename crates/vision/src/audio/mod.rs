//! Audio processing capabilities

pub mod transcription;
pub mod tts;

pub use transcription::AudioTranscriber;
pub use tts::TextToSpeech;
