//! Sound output on macOS, through CoreAudio: an output AudioUnit per
//! stream, Apple's default one (which follows the system's default device)
//! or one bound to a chosen device. Its render callback reads from a queue
//! without ever waiting; `write` naps while the queue is full. Devices come
//! from the HAL: settings keep a device's UID, which stays the same across
//! reboots, and people read its name.

use super::Device;
use objc2_audio_toolbox::{
    kAudioOutputUnitProperty_CurrentDevice, kAudioUnitManufacturer_Apple, kAudioUnitProperty_Latency, kAudioUnitProperty_SetRenderCallback,
    kAudioUnitProperty_StreamFormat, kAudioUnitScope_Global, kAudioUnitScope_Input, kAudioUnitSubType_DefaultOutput,
    kAudioUnitSubType_HALOutput, kAudioUnitType_Output, AURenderCallbackStruct, AudioComponentDescription, AudioComponentFindNext,
    AudioComponentInstance, AudioComponentInstanceDispose, AudioComponentInstanceNew, AudioOutputUnitStart, AudioOutputUnitStop,
    AudioUnitGetProperty, AudioUnitInitialize, AudioUnitRenderActionFlags, AudioUnitSetProperty, AudioUnitUninitialize,
};
use objc2_core_audio::{
    kAudioDevicePropertyBufferFrameSize, kAudioDevicePropertyDeviceUID, kAudioDevicePropertyLatency, kAudioDevicePropertyNominalSampleRate,
    kAudioDevicePropertySafetyOffset, kAudioDevicePropertyStreams, kAudioHardwarePropertyDefaultOutputDevice,
    kAudioHardwarePropertyDevices, kAudioObjectPropertyElementMain, kAudioObjectPropertyName, kAudioObjectPropertyScopeGlobal,
    kAudioObjectPropertyScopeOutput, kAudioObjectSystemObject, kAudioObjectUnknown, AudioObjectGetPropertyData,
    AudioObjectGetPropertyDataSize, AudioObjectID, AudioObjectPropertyAddress,
};
use objc2_core_audio_types::{
    kAudioFormatFlagIsFloat, kAudioFormatFlagIsPacked, kAudioFormatLinearPCM, AudioBufferList, AudioStreamBasicDescription, AudioTimeStamp,
};
use objc2_core_foundation::{CFRetained, CFString};
use parking_lot::Mutex;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

/// How long a device may go without asking for sound, while sound waits,
/// before it counts as gone.
const STALLED: Duration = Duration::from_secs(1);

/// An open, blocking, interleaved playback stream.
pub struct CoreAudioPcm {
    /// The device's stream; none for `null`.
    stream: Option<Stream>,
    pub rate: u32,
    pub channels: u32,
}

impl CoreAudioPcm {
    /// Opens the device with that UID, the default one, or `null`.
    pub fn open(device: &str, rate: u32, channels: u32, latency_ms: u32) -> Result<Self, String> {
        let stream = match device {
            "null" => None,
            super::DEFAULT => Some(Stream::open(None, rate, channels, latency_ms)?),
            uid => {
                let id = device_by_uid(uid).ok_or_else(|| format!("no output device {uid}"))?;
                Some(Stream::open(Some(id), rate, channels, latency_ms)?)
            }
        };
        Ok(Self { stream, rate, channels })
    }

    /// Writes interleaved samples, blocking until the device took them.
    pub fn write(&mut self, samples: &[f32]) -> Result<(), String> {
        self.stream.as_mut().map_or(Ok(()), |s| s.write(samples))
    }

    /// Frames queued but not yet audible.
    pub fn delay(&self) -> u64 {
        self.stream.as_ref().map_or(0, Stream::delay)
    }

    /// Pauses or resumes, keeping what is queued.
    pub fn pause(&mut self, on: bool) -> bool {
        self.stream.as_mut().is_none_or(|s| s.pause(on))
    }

    /// Discards queued audio; the next write starts afresh.
    pub fn discard(&mut self) {
        if let Some(s) = &mut self.stream {
            s.discard();
        }
    }

    /// Blocks until queued audio has played.
    pub fn drain(&mut self) {
        if let Some(s) = &self.stream {
            s.drain();
        }
    }
}

/// What the render callback reads: the queue's far end. `discard` locks it
/// to empty the queue; the callback only tries the lock, and plays silence
/// while it is taken, so it never waits. `calls` counts the callbacks, so
/// `write` can tell a device that stopped asking for sound.
struct Shared {
    consumer: Mutex<rtrb::Consumer<f32>>,
    calls: AtomicU32,
}

/// A device's output unit and the queue it plays from.
struct Stream {
    unit: AudioComponentInstance,
    /// Owned here, and freed only after the unit is gone (see `Drop`).
    shared: NonNull<Shared>,
    producer: rtrb::Producer<f32>,
    capacity: usize,
    channels: usize,
    rate: u32,
    /// What the device and the unit hold before a frame is heard.
    latency_frames: u64,
    /// How long `write` and `drain` wait for room: a quarter of the
    /// device's buffer.
    nap: Duration,
    paused: bool,
}

// SAFETY: the engine uses the stream from one thread at a time, as it does
// `AlsaPcm`; the callback reaches only `Shared`, which is `Sync`.
unsafe impl Send for Stream {}

impl Stream {
    /// Opens `device`, or the system's default device (followed when it
    /// changes) for `None`, and starts it playing silence.
    fn open(device: Option<AudioObjectID>, rate: u32, channels: u32, latency_ms: u32) -> Result<Stream, String> {
        let fail = |status: i32| format!("CoreAudio cannot play {rate} Hz / {channels} ch: error {status}");
        let check = |status: i32| if status == 0 { Ok(()) } else { Err(fail(status)) };
        let description = AudioComponentDescription {
            componentType: kAudioUnitType_Output,
            componentSubType: if device.is_some() { kAudioUnitSubType_HALOutput } else { kAudioUnitSubType_DefaultOutput },
            componentManufacturer: kAudioUnitManufacturer_Apple,
            componentFlags: 0,
            componentFlagsMask: 0,
        };
        // SAFETY: a filled description; null asks for the first match.
        let component = unsafe { AudioComponentFindNext(std::ptr::null_mut(), NonNull::from(&description)) };
        if component.is_null() {
            return Err("CoreAudio has no output unit".into());
        }
        let mut unit: AudioComponentInstance = std::ptr::null_mut();
        // SAFETY: `component` is a found component; `unit` receives the instance.
        check(unsafe { AudioComponentInstanceNew(component, NonNull::from(&mut unit)) })?;

        let heard_on = device.or_else(default_output);
        let (device_frames, device_rate) = heard_on.map_or((0, 0.0), device_latency);
        let buffer = heard_on
            .and_then(|id| property::<u32>(id, kAudioDevicePropertyBufferFrameSize, kAudioObjectPropertyScopeGlobal))
            .unwrap_or(512);
        let capacity = ring_capacity(rate, channels, latency_ms, buffer);
        let (producer, consumer) = rtrb::RingBuffer::new(capacity);
        let shared = NonNull::from(Box::leak(Box::new(Shared { consumer: Mutex::new(consumer), calls: AtomicU32::new(0) })));
        let nap = match device_rate {
            r if r > 0.0 => Duration::from_secs_f64(buffer as f64 / r / 4.0).clamp(Duration::from_millis(1), Duration::from_millis(10)),
            _ => Duration::from_millis(5),
        };
        // From here on, dropping `stream` stops and frees all of it.
        let mut stream =
            Stream { unit, shared, producer, capacity, channels: channels as usize, rate, latency_frames: 0, nap, paused: false };

        let set = |id: u32, scope: u32, data: *const c_void, size: usize| {
            // SAFETY: `data` points at `size` bytes of the property's type.
            check(unsafe { AudioUnitSetProperty(unit, id, scope, 0, data, size as u32) })
        };
        if let Some(id) = device {
            set(
                kAudioOutputUnitProperty_CurrentDevice,
                kAudioUnitScope_Global,
                (&id as *const AudioObjectID).cast(),
                size_of::<AudioObjectID>(),
            )?;
        }
        let bytes = 4 * channels;
        let format = AudioStreamBasicDescription {
            mSampleRate: rate as f64,
            mFormatID: kAudioFormatLinearPCM,
            mFormatFlags: kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked,
            mBytesPerPacket: bytes,
            mFramesPerPacket: 1,
            mBytesPerFrame: bytes,
            mChannelsPerFrame: channels,
            mBitsPerChannel: 32,
            mReserved: 0,
        };
        set(
            kAudioUnitProperty_StreamFormat,
            kAudioUnitScope_Input,
            (&format as *const AudioStreamBasicDescription).cast(),
            size_of_val(&format),
        )?;
        let callback = AURenderCallbackStruct { inputProc: Some(render), inputProcRefCon: shared.as_ptr().cast() };
        set(
            kAudioUnitProperty_SetRenderCallback,
            kAudioUnitScope_Input,
            (&callback as *const AURenderCallbackStruct).cast(),
            size_of_val(&callback),
        )?;
        // SAFETY: a configured unit of ours.
        check(unsafe { AudioUnitInitialize(unit) })?;

        let mut unit_latency = 0.0f64;
        let mut size = size_of::<f64>() as u32;
        // SAFETY: the property is one f64, written into `unit_latency`.
        let read = unsafe {
            AudioUnitGetProperty(
                unit,
                kAudioUnitProperty_Latency,
                kAudioUnitScope_Global,
                0,
                NonNull::from(&mut unit_latency).cast(),
                NonNull::from(&mut size),
            )
        };
        let unit_frames = if read == 0 { (unit_latency * rate as f64).round() as u64 } else { 0 };
        stream.latency_frames = to_stream_frames(device_frames, device_rate, rate) + unit_frames;
        // SAFETY: an initialized unit of ours.
        check(unsafe { AudioOutputUnitStart(unit) })?;
        Ok(stream)
    }

    /// Queues `samples`, napping while the queue is full. A device that has
    /// stopped asking for sound for a second (unplugged, asleep) is an
    /// error, as ALSA and WASAPI report one, so the engine can move on.
    fn write(&mut self, samples: &[f32]) -> Result<(), String> {
        // SAFETY: `shared` lives as long as the stream.
        let calls = &unsafe { self.shared.as_ref() }.calls;
        let (mut seen, mut since) = (calls.load(Ordering::Relaxed), Instant::now());
        let mut rest = samples;
        while !rest.is_empty() {
            let room = self.producer.slots().min(rest.len());
            if room == 0 {
                if self.paused {
                    return Err("cannot queue sound on a paused device".into());
                }
                let now = calls.load(Ordering::Relaxed);
                if now != seen {
                    (seen, since) = (now, Instant::now());
                } else if since.elapsed() >= STALLED {
                    return Err("the sound device stopped playing".into());
                }
                std::thread::sleep(self.nap);
                continue;
            }
            let chunk = self.producer.write_chunk_uninit(room).map_err(|e| e.to_string())?;
            chunk.fill_from_iter(rest[..room].iter().copied());
            rest = &rest[room..];
        }
        Ok(())
    }

    /// The frames in the queue.
    fn queued(&self) -> usize {
        (self.capacity - self.producer.slots()) / self.channels
    }

    /// The queued frames and what the device holds; none once the queue is empty.
    fn delay(&self) -> u64 {
        match self.queued() {
            0 => 0,
            frames => frames as u64 + self.latency_frames,
        }
    }

    fn pause(&mut self, on: bool) -> bool {
        // SAFETY: a started unit of ours; stopping keeps its state.
        let status = unsafe {
            if on {
                AudioOutputUnitStop(self.unit)
            } else {
                AudioOutputUnitStart(self.unit)
            }
        };
        if status == 0 {
            self.paused = on;
        }
        status == 0
    }

    /// Empties the queue and, like ALSA's drop and prepare, ends a pause:
    /// what is written next plays.
    fn discard(&mut self) {
        {
            // SAFETY: `shared` lives as long as the stream.
            let mut consumer = unsafe { self.shared.as_ref() }.consumer.lock();
            let all = consumer.slots();
            if let Ok(chunk) = consumer.read_chunk(all) {
                chunk.commit_all();
            }
        }
        if self.paused {
            self.pause(false);
        }
    }

    /// Waits until the queue has played and been heard; at once while paused.
    fn drain(&self) {
        if self.paused {
            return;
        }
        let deadline = Instant::now() + Duration::from_secs_f64(self.queued() as f64 / self.rate as f64) + Duration::from_secs(1);
        while self.queued() > 0 && Instant::now() < deadline {
            std::thread::sleep(self.nap);
        }
        std::thread::sleep(Duration::from_secs_f64(self.latency_frames as f64 / self.rate as f64));
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        // SAFETY: the unit is ours; once it is stopped and disposed the
        // callback no longer runs, so `shared` can go after it.
        unsafe {
            AudioOutputUnitStop(self.unit);
            AudioUnitUninitialize(self.unit);
            AudioComponentInstanceDispose(self.unit);
            drop(Box::from_raw(self.shared.as_ptr()));
        }
    }
}

/// Fills the unit's buffer from the queue, and with silence for whatever
/// the queue does not have. Runs on the audio thread: no waiting, no
/// allocating.
unsafe extern "C-unwind" fn render(
    shared: NonNull<c_void>,
    _flags: NonNull<AudioUnitRenderActionFlags>,
    _time: NonNull<AudioTimeStamp>,
    _bus: u32,
    _frames: u32,
    data: *mut AudioBufferList,
) -> i32 {
    // SAFETY: `shared` is the `Shared` the stream gave the unit, alive
    // until the unit is disposed; `data` is the unit's own buffer list.
    let shared = unsafe { shared.cast::<Shared>().as_ref() };
    shared.calls.fetch_add(1, Ordering::Relaxed);
    let Some(list) = (unsafe { data.as_mut() }) else { return 0 };
    // SAFETY: the list holds `mNumberBuffers` buffers.
    let buffers = unsafe { std::slice::from_raw_parts_mut(list.mBuffers.as_mut_ptr(), list.mNumberBuffers as usize) };
    for buffer in buffers {
        let Some(start) = NonNull::new(buffer.mData.cast::<f32>()) else { continue };
        // SAFETY: the buffer is `mDataByteSize` bytes of f32 samples.
        let out = unsafe { std::slice::from_raw_parts_mut(start.as_ptr(), buffer.mDataByteSize as usize / size_of::<f32>()) };
        let mut filled = 0;
        if let Some(mut consumer) = shared.consumer.try_lock() {
            let n = consumer.slots().min(out.len());
            if let Ok(chunk) = consumer.read_chunk(n) {
                let (first, second) = chunk.as_slices();
                out[..first.len()].copy_from_slice(first);
                out[first.len()..n].copy_from_slice(second);
                chunk.commit_all();
                filled = n;
            }
        }
        out[filled..].fill(0.0);
    }
    0
}

/// The queue's size in samples: the latency asked for, but at least two of
/// the device's buffers so the callback never finds it short by design.
fn ring_capacity(rate: u32, channels: u32, latency_ms: u32, device_buffer_frames: u32) -> usize {
    let frames = (rate as u64 * latency_ms as u64 / 1000).max(2 * device_buffer_frames as u64);
    frames as usize * channels as usize
}

/// Whether there is a device to play on.
pub fn available() -> Result<(), String> {
    default_output().map(|_| ()).ok_or_else(|| "no sound output device".into())
}

/// The devices that can play, by name, each kept by its UID.
pub fn devices() -> Vec<Device> {
    output_devices().into_iter().map(|(_, uid, name)| Device { name: uid, label: name }).collect()
}

/// The address of property `selector` in `scope`.
fn address(selector: u32, scope: u32) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress { mSelector: selector, mScope: scope, mElement: kAudioObjectPropertyElementMain }
}

/// A fixed-size property of `object`.
fn property<T: Copy + Default>(object: AudioObjectID, selector: u32, scope: u32) -> Option<T> {
    let address = address(selector, scope);
    let mut value = T::default();
    let mut size = size_of::<T>() as u32;
    // SAFETY: `value` is a `T` of `size` bytes, and the HAL writes no more.
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&address),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
            NonNull::from(&mut value).cast(),
        )
    };
    (status == 0).then_some(value)
}

/// A string property of `object`.
fn string_property(object: AudioObjectID, selector: u32) -> Option<String> {
    let address = address(selector, kAudioObjectPropertyScopeGlobal);
    let mut string: *const CFString = std::ptr::null();
    let mut size = size_of::<*const CFString>() as u32;
    // SAFETY: the HAL writes one CFString reference into `string`.
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&address),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
            NonNull::from(&mut string).cast(),
        )
    };
    if status != 0 {
        return None;
    }
    // SAFETY: the HAL hands over a reference of its own (+1), released here.
    let string = unsafe { CFRetained::from_raw(NonNull::new(string.cast_mut())?) };
    Some(string.to_string())
}

/// How many bytes property `selector` of `object` has in `scope`.
fn property_size(object: AudioObjectID, selector: u32, scope: u32) -> u32 {
    let address = address(selector, scope);
    let mut size = 0;
    // SAFETY: only writes `size`.
    let status = unsafe { AudioObjectGetPropertyDataSize(object, NonNull::from(&address), 0, std::ptr::null(), NonNull::from(&mut size)) };
    if status == 0 {
        size
    } else {
        0
    }
}

/// The system's default output device, if there is one.
fn default_output() -> Option<AudioObjectID> {
    property(kAudioObjectSystemObject as AudioObjectID, kAudioHardwarePropertyDefaultOutputDevice, kAudioObjectPropertyScopeGlobal)
        .filter(|&id| id != kAudioObjectUnknown)
}

/// Every device that can play: its ID, UID and name.
fn output_devices() -> Vec<(AudioObjectID, String, String)> {
    let system = kAudioObjectSystemObject as AudioObjectID;
    let address = address(kAudioHardwarePropertyDevices, kAudioObjectPropertyScopeGlobal);
    let mut size = property_size(system, kAudioHardwarePropertyDevices, kAudioObjectPropertyScopeGlobal);
    let mut ids = vec![kAudioObjectUnknown; size as usize / size_of::<AudioObjectID>()];
    let Some(out) = NonNull::new(ids.as_mut_ptr()) else { return Vec::new() };
    // SAFETY: `ids` has room for `size` bytes of device IDs.
    if unsafe { AudioObjectGetPropertyData(system, NonNull::from(&address), 0, std::ptr::null(), NonNull::from(&mut size), out.cast()) }
        != 0
    {
        return Vec::new();
    }
    ids.truncate(size as usize / size_of::<AudioObjectID>());
    ids.into_iter()
        .filter(|&id| property_size(id, kAudioDevicePropertyStreams, kAudioObjectPropertyScopeOutput) > 0)
        .filter_map(|id| Some((id, string_property(id, kAudioDevicePropertyDeviceUID)?, string_property(id, kAudioObjectPropertyName)?)))
        .collect()
}

/// The device with UID `uid`, if it is there and can play.
fn device_by_uid(uid: &str) -> Option<AudioObjectID> {
    output_devices().into_iter().find(|(_, u, _)| u == uid).map(|(id, _, _)| id)
}

/// How many frames `device` holds before they are heard (its latency, its
/// safety offset and its buffer), at its own rate, which comes second.
fn device_latency(device: AudioObjectID) -> (u32, f64) {
    let out = |selector| property::<u32>(device, selector, kAudioObjectPropertyScopeOutput).unwrap_or(0);
    let buffer = property::<u32>(device, kAudioDevicePropertyBufferFrameSize, kAudioObjectPropertyScopeGlobal).unwrap_or(0);
    let rate = property::<f64>(device, kAudioDevicePropertyNominalSampleRate, kAudioObjectPropertyScopeGlobal).unwrap_or(0.0);
    (out(kAudioDevicePropertyLatency) + out(kAudioDevicePropertySafetyOffset) + buffer, rate)
}

/// `device_frames` at `device_rate`, counted at `rate`; 0 for a device
/// that says nothing of its rate.
fn to_stream_frames(device_frames: u32, device_rate: f64, rate: u32) -> u64 {
    if device_rate <= 0.0 {
        return 0;
    }
    (device_frames as f64 * rate as f64 / device_rate).round() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_takes_samples_and_plays_nothing() {
        let mut pcm = CoreAudioPcm::open("null", 44_100, 2, 100).unwrap();
        assert_eq!(pcm.write(&[0.0; 512]), Ok(()));
        assert_eq!((pcm.delay(), pcm.rate, pcm.channels), (0, 44_100, 2));
    }

    /// Whether this Mac can play sound; hardware tests skip themselves otherwise.
    fn speakers() -> bool {
        let there = default_output().is_some();
        if !there {
            eprintln!("no sound output device: skipped");
        }
        there
    }

    #[test]
    fn device_frames_count_at_the_streams_rate() {
        assert_eq!(to_stream_frames(480, 48_000.0, 44_100), 441);
        assert_eq!(to_stream_frames(512, 44_100.0, 44_100), 512);
        assert_eq!(to_stream_frames(0, 0.0, 44_100), 0); // a device that says nothing
    }

    #[test]
    fn devices_have_a_name_people_read_and_a_uid_settings_keep() {
        if !speakers() {
            return;
        }
        let list = devices();
        assert!(!list.is_empty() && available().is_ok());
        assert!(list.iter().all(|d| !d.label.is_empty() && !d.name.is_empty() && d.name != super::super::DEFAULT));
        let mut uids: Vec<&str> = list.iter().map(|d| d.name.as_str()).collect();
        uids.dedup();
        assert_eq!(uids.len(), list.len(), "{list:?}");
    }

    #[test]
    fn a_device_opens_by_its_uid() {
        if !speakers() {
            return;
        }
        let first = devices().remove(0);
        assert!(CoreAudioPcm::open(&first.name, 48_000, 2, 150).is_ok(), "{first:?}");
    }

    #[test]
    fn an_unknown_device_is_named_in_the_error() {
        let e = CoreAudioPcm::open("popamp-no-such-uid", 48_000, 2, 150).err().unwrap();
        assert!(e.contains("popamp-no-such-uid"), "{e}");
    }

    #[test]
    fn the_queue_holds_the_latency_asked_for_or_two_device_buffers() {
        assert_eq!(ring_capacity(48_000, 2, 150, 512), 7_200 * 2);
        assert_eq!(ring_capacity(48_000, 2, 10, 512), 1_024 * 2);
    }

    /// `ms` of stereo silence at `rate`.
    fn silence(rate: u32, ms: u32) -> Vec<f32> {
        vec![0.0; (rate * ms / 1000 * 2) as usize]
    }

    #[test]
    fn the_default_device_plays_in_real_time() {
        if !speakers() {
            return;
        }
        // 44.1 kHz on what is usually a 48 kHz device: the unit converts.
        let mut pcm = CoreAudioPcm::open(super::super::DEFAULT, 44_100, 2, 100).unwrap();
        let started = std::time::Instant::now();
        for _ in 0..10 {
            pcm.write(&silence(44_100, 50)).unwrap();
        }
        let took = started.elapsed();
        assert!(took >= std::time::Duration::from_millis(300) && took < std::time::Duration::from_secs(2), "{took:?}");
    }

    #[test]
    fn delay_counts_what_is_queued() {
        if !speakers() {
            return;
        }
        let mut pcm = CoreAudioPcm::open(super::super::DEFAULT, 48_000, 2, 150).unwrap();
        assert!(pcm.pause(true));
        pcm.write(&silence(48_000, 50)).unwrap();
        assert!(pcm.delay() >= 2_400, "{}", pcm.delay());
    }

    #[test]
    fn discard_empties_the_queue() {
        if !speakers() {
            return;
        }
        let mut pcm = CoreAudioPcm::open(super::super::DEFAULT, 48_000, 2, 150).unwrap();
        pcm.pause(true);
        pcm.write(&silence(48_000, 50)).unwrap();
        pcm.discard();
        assert_eq!(pcm.delay(), 0);
    }

    #[test]
    fn drain_waits_until_everything_is_heard() {
        if !speakers() {
            return;
        }
        let mut pcm = CoreAudioPcm::open(super::super::DEFAULT, 48_000, 2, 150).unwrap();
        pcm.write(&silence(48_000, 100)).unwrap();
        let started = std::time::Instant::now();
        pcm.drain();
        assert!(started.elapsed() >= std::time::Duration::from_millis(50), "{:?}", started.elapsed());
        assert_eq!(pcm.delay(), 0);
    }

    // A device that goes away stops calling back; writing must not wait for it for ever.
    #[test]
    fn a_device_that_stops_playing_is_reported() {
        if !speakers() {
            return;
        }
        let mut pcm = CoreAudioPcm::open(super::super::DEFAULT, 48_000, 2, 100).unwrap();
        let stream = pcm.stream.as_ref().unwrap();
        // SAFETY: stops our own unit behind the stream's back, as a lost device would.
        unsafe { AudioOutputUnitStop(stream.unit) };
        let started = Instant::now();
        let e = pcm.write(&silence(48_000, 300)).err().unwrap();
        assert!(e.contains("stopped playing"), "{e}");
        assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
    }
}
