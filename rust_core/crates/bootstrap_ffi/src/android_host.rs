//! Gives CPAL's AAudio backend the Android activity context.
//!
//! `ndk-context` must be initialized once, before playback asks for a device.
//! The activity reference is promoted to a global ref so it outlives this call.

use std::os::raw::c_void;
use std::sync::atomic::{AtomicBool, Ordering};

static CONTEXT_READY: AtomicBool = AtomicBool::new(false);

/// Called from `MainActivity` before the Flutter engine opens the bridge.
///
/// # Safety
/// `env` and `context` are the live JNI pointers for this call. `context` is a
/// local reference owned by the JNI frame; this function does not delete it.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_matiusjbg_atomusicator_MainActivity_nativeInit(
    env: *mut jni::sys::JNIEnv,
    _class: jni::sys::jclass,
    context: jni::sys::jobject,
) {
    if CONTEXT_READY.load(Ordering::Acquire) || context.is_null() || env.is_null() {
        return;
    }
    let mut env = match unsafe { jni::JNIEnv::from_raw(env) } {
        Ok(env) => env,
        Err(_) => return,
    };
    let vm = match env.get_java_vm() {
        Ok(vm) => vm,
        Err(_) => return,
    };
    let local = unsafe { jni::objects::JObject::from_raw(context) };
    let files = files_directory(&mut env, &local);
    let global = env.new_global_ref(&local);
    // The incoming reference is local to this JNI frame. Dropping the wrapper
    // would delete it before the caller returns, so the frame keeps ownership.
    std::mem::forget(local);
    let Ok(global) = global else {
        return;
    };
    let context_ptr = global.as_obj().as_raw();
    std::mem::forget(global);
    unsafe {
        ndk_context::initialize_android_context(
            vm.get_java_vm_pointer().cast::<c_void>(),
            context_ptr.cast::<c_void>(),
        );
    }
    std::mem::forget(vm);
    if let Some(files) = files {
        // SAFETY: nativeInit runs on the main thread before other engine threads exist.
        unsafe { std::env::set_var("ATOMUSICATOR_APP_DATA", files) };
    }
    CONTEXT_READY.store(true, Ordering::Release);
}

fn files_directory(env: &mut jni::JNIEnv, context: &jni::objects::JObject) -> Option<String> {
    let files = env
        .call_method(context, "getFilesDir", "()Ljava/io/File;", &[])
        .ok()?
        .l()
        .ok()?;
    let path = env
        .call_method(&files, "getAbsolutePath", "()Ljava/lang/String;", &[])
        .ok()?
        .l()
        .ok()?;
    let text = jni::objects::JString::from(path);
    let text = env.get_string(&text).ok()?;
    Some(text.to_string_lossy().into_owned())
}

static MODELS_READY: (std::sync::Mutex<bool>, std::sync::Condvar) =
    (std::sync::Mutex::new(false), std::sync::Condvar::new());

#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_matiusjbg_atomusicator_MainActivity_nativeModelsReady(
    _env: *mut jni::sys::JNIEnv,
    _class: jni::sys::jclass,
) {
    let (lock, cvar) = &MODELS_READY;
    if let Ok(mut ready) = lock.lock() {
        *ready = true;
        cvar.notify_all();
    }
}

pub fn wait_for_bundled_models() {
    let (lock, cvar) = &MODELS_READY;
    if let Ok(mut ready) = lock.lock() {
        while !*ready {
            let Ok((guard, timeout)) =
                cvar.wait_timeout(ready, std::time::Duration::from_secs(60))
            else {
                break;
            };
            ready = guard;
            if timeout.timed_out() {
                break;
            }
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_matiusjbg_atomusicator_MainActivity_nativePreviewAudio(
    env: *mut jni::sys::JNIEnv,
    _class: jni::sys::jclass,
    path: jni::sys::jstring,
    buckets: jni::sys::jint,
) -> jni::sys::jstring {
    if env.is_null() || path.is_null() {
        return std::ptr::null_mut();
    }
    let mut env = match unsafe { jni::JNIEnv::from_raw(env) } {
        Ok(env) => env,
        Err(_) => return std::ptr::null_mut(),
    };
    let jpath = unsafe { jni::objects::JString::from_raw(path) };
    let path_str = env
        .get_string(&jpath)
        .ok()
        .map(|s| s.to_string_lossy().into_owned());
    std::mem::forget(jpath);
    let Some(path_str) = path_str else {
        return std::ptr::null_mut();
    };
    let cancel = atomusicator_application::CancellationToken::default();
    let preview = match atomusicator_infrastructure::symphonia_decode::preview_audio(
        std::path::Path::new(&path_str),
        buckets.max(16) as usize,
        &cancel,
    ) {
        Ok(p) => p,
        Err(_) => return std::ptr::null_mut(),
    };
    let Ok(json) = serde_json::to_string(&preview) else {
        return std::ptr::null_mut();
    };
    match env.new_string(json) {
        Ok(jstr) => jstr.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn Java_com_matiusjbg_atomusicator_MainActivity_nativeVerifyPipeline(
    env: *mut jni::sys::JNIEnv,
    _class: jni::sys::jclass,
) -> jni::sys::jstring {
    if env.is_null() {
        return std::ptr::null_mut();
    }
    let env = match unsafe { jni::JNIEnv::from_raw(env) } {
        Ok(env) => env,
        Err(_) => return std::ptr::null_mut(),
    };
    let report = match run_e2e_pipeline_verification() {
        Ok(msg) => format!("E2E_OK: {msg}"),
        Err(err) => format!("E2E_ERR: {err}"),
    };
    match env.new_string(report) {
        Ok(jstr) => jstr.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

fn run_e2e_pipeline_verification() -> Result<String, String> {
    use atomusicator_domain::{AudioBufferF32, AudioFormat};
    use atomusicator_infrastructure::{
        audio_io::wav_codec::write_wav_f32,
        audio_tools::ExportOptions,
        storage::paths::WorkspacePathPolicy,
    };
    use std::time::{Duration, Instant};

    wait_for_bundled_models();
    let policy = WorkspacePathPolicy::system_default().map_err(|e| e.to_string())?;
    let studio = crate::studio::studio()?;

    // 1. Create a 1.5-second stereo 44.1 kHz test audio file in imports/e2e/
    let import_dir = policy.app_data_root().join("e2e-import");
    std::fs::create_dir_all(&import_dir).map_err(|e| e.to_string())?;
    let test_wav = import_dir.join("Prueba E2E.wav");
    let samples: Vec<f32> = (0..(44100 + 22050))
        .flat_map(|i| {
            let t = i as f32 / 44100.0;
            let l = (t * 440.0 * std::f32::consts::TAU).sin() * 0.25;
            let r = (t * 660.0 * std::f32::consts::TAU).sin() * 0.20;
            [l, r]
        })
        .collect();
    let buf = AudioBufferF32::new(AudioFormat::new(44100, 2).map_err(|e| e.to_string())?, samples)
        .map_err(|e| e.to_string())?;
    write_wav_f32(&test_wav, &buf).map_err(|e| e.to_string())?;

    // 2. Verify native Symphonia audio preview
    let preview = atomusicator_infrastructure::symphonia_decode::preview_audio(
        &test_wav,
        650,
        &atomusicator_application::CancellationToken::default(),
    )
    .map_err(|e| format!("preview_audio failed: {e}"))?;
    if preview.sample_rate != 44100 || preview.channels != 2 || preview.peaks.len() != 1300 {
        return Err(format!("invalid preview: sr={}, ch={}, peaks={}", preview.sample_rate, preview.channels, preview.peaks.len()));
    }

    // 3. Run full 5-stem separation using cached ONNX models (allow_download = false proves offline models work!)
    let sep_id = studio.separate(test_wav.clone(), false)?;
    let deadline = Instant::now() + Duration::from_secs(180);
    let sep_job = loop {
        let job = studio.job(&sep_id)?;
        if !matches!(
            job.status,
            crate::api::UiJobStatus::Queued
                | crate::api::UiJobStatus::Running
                | crate::api::UiJobStatus::Committing
        ) {
            break job;
        }
        if Instant::now() > deadline {
            return Err("separation timed out".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    if sep_job.status != crate::api::UiJobStatus::Completed {
        return Err(format!("separation failed ({:?}): {}", sep_job.status, sep_job.message));
    }
    let session = sep_job.session.ok_or("missing session in completed separation")?;
    if session.tracks.len() != 5 {
        return Err(format!("expected 5 stems, got {}", session.tracks.len()));
    }

    // 4. Verify saved project in projects/ directory & session.json schema
    let project_dir = std::path::PathBuf::from(&session.directory);
    let session_json_bytes = std::fs::read(project_dir.join("session.json")).map_err(|e| e.to_string())?;
    let session_json: serde_json::Value = serde_json::from_slice(&session_json_bytes).map_err(|e| e.to_string())?;
    if session_json.pointer("/separation/state").and_then(|v| v.as_str()) != Some("complete")
        || session_json.pointer("/separation/status").and_then(|v| v.as_str()) != Some("complete")
    {
        return Err("session.json missing complete state/status".into());
    }

    // 5. Reopen project into Mixer and test track controls, live pitch, and playback
    let reopened = studio.open(&project_dir)?;
    if reopened.name != "Prueba E2E.wav" || reopened.tracks.len() != 5 {
        return Err(format!("unexpected reopened project: name={}, tracks={}", reopened.name, reopened.tracks.len()));
    }
    crate::api::set_track("lead_vocal".into(), -2.0, -0.25, false, false)?;
    let pitch_state = studio.live_pitch_status()?;
    let pitched = studio.set_live_pitch(&reopened.id, pitch_state.epoch, 1, 2)?;
    if pitched.semitones != 2 {
        return Err("set_live_pitch did not apply semitones=2".into());
    }
    let _ = crate::api::play();
    std::thread::sleep(Duration::from_millis(120));
    let _ = crate::api::seek(0.4);
    let _ = crate::api::pause();

    // 6. Export stems to exports/ directory
    let exports_dir = policy.app_data_root().join("exports");
    std::fs::create_dir_all(&exports_dir).map_err(|e| e.to_string())?;
    let exp_id = studio.export_with_pitch(
        exports_dir.clone(),
        vec!["lead_vocal".into(), "drums".into(), "instrumental".into()],
        ExportOptions::new("wav", 44100, 2, 24, false).map_err(|e| e.to_string())?,
        None,
    )?;
    let exp_job = loop {
        let job = studio.job(&exp_id)?;
        if !matches!(
            job.status,
            crate::api::UiJobStatus::Queued
                | crate::api::UiJobStatus::Running
                | crate::api::UiJobStatus::Committing
        ) {
            break job;
        }
        if Instant::now() > deadline {
            return Err("export timed out".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    if exp_job.status != crate::api::UiJobStatus::Completed {
        return Err(format!("export failed ({:?}): {}", exp_job.status, exp_job.message));
    }
    let exp_out = std::path::PathBuf::from(exp_job.output_directory.ok_or("missing export output directory")?);
    for stem in ["lead_vocal.wav", "drums.wav", "instrumental.wav"] {
        if !exp_out.join(stem).is_file() {
            return Err(format!("missing exported stem file: {stem}"));
        }
    }

    // Clean up temporary E2E artifacts so user workspace stays clean
    let _ = studio.close_session();
    let _ = std::fs::remove_dir_all(&project_dir);
    let _ = std::fs::remove_dir_all(&exp_out);
    let _ = std::fs::remove_dir_all(&import_dir);

    Ok(format!(
        "preview=ok(1300 peaks), separation=ok(5 stems), project_save_reopen=ok({}), live_pitch=ok(+2st), export=ok(3 wavs)",
        reopened.name
    ))
}
