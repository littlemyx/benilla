//! The audio session: the Playback category, so sound ignores the silent switch.

use objc2_avf_audio::{AVAudioSession, AVAudioSessionCategoryPlayback};

/// Set the shared session to the Playback category and activate it. Call before the output
/// device opens; the error is the `NSError` description.
pub fn activate_playback_session() -> Result<(), String> {
    // SAFETY: `sharedInstance` is thread-safe, `AVAudioSessionCategoryPlayback` is an
    // immutable extern constant (None only on an OS that lacks it), and the setters take no
    // out-of-lifetime pointers.
    unsafe {
        let session = AVAudioSession::sharedInstance();
        let category =
            AVAudioSessionCategoryPlayback.ok_or("AVAudioSessionCategoryPlayback missing")?;
        session
            .setCategory_error(category)
            .map_err(|e| e.localizedDescription().to_string())?;
        session
            .setActive_error(true)
            .map_err(|e| e.localizedDescription().to_string())
    }
}
