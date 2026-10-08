//! Audio processing capabilities

pub mod model_manager;
pub mod transcription;
pub mod tts;

pub use transcription::AudioTranscriber;
pub use tts::TextToSpeech;
