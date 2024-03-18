use crate::regulator::{audio_regulator_new, AudioRegulatorPull, AudioRegulatorPush};

pub struct KyAudioRegulator {
    push: AudioRegulatorPush,
    pull: AudioRegulatorPull,
}

#[no_mangle]
pub unsafe extern "C" fn kyaudioreg_new(
    sample_rate: u32,
    sample_size: u32,
    capacity: u32,
    target_buffering: u32,
) -> Box<KyAudioRegulator> {
    let (push, pull) = audio_regulator_new(sample_rate, sample_size, capacity, target_buffering);
    Box::new(KyAudioRegulator { push, pull })
}

#[no_mangle]
#[allow(unused_variables)]
pub unsafe extern "C" fn kyaudioreg_delete(audioreg: Box<KyAudioRegulator>) {
    // implicitly drop audioreg
}

#[no_mangle]
pub unsafe extern "C" fn kyaudioreg_push(
    audioreg: &mut KyAudioRegulator,
    src: *const u8,
    len: libc::size_t,
) {
    let data = std::slice::from_raw_parts(src, len as usize);
    audioreg.push.push(data);
}

#[no_mangle]
pub unsafe extern "C" fn kyaudioreg_pull(
    audioreg: &mut KyAudioRegulator,
    dst: *mut u8,
    len: libc::size_t,
) {
    let data = std::slice::from_raw_parts_mut(dst, len as usize);
    audioreg.pull.pull(data);
}
