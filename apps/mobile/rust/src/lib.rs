pub mod api;
mod frb_generated;

/// Initialize blew's Android JNI layer.
///
/// Called from Kotlin `BlewPlugin.onAttachedToEngine` which passes
/// `applicationContext`. Uses raw JNI function table calls instead of
/// jni 0.22's `EnvUnowned::with_env` (which silently fails in release).
///
/// Kotlin must call `BleCentralManager.init(ctx)` and
/// `BlePeripheralManager.init(ctx)` BEFORE this JNI call so blew's
/// permission check sees a non-null context. The `proguard-rules.pro`
/// keep rule for `org.jakebot.blew.**` is also required — without it
/// R8 obfuscates the method names that blew resolves via JNI strings.
#[cfg(target_os = "android")]
mod init_android {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicPtr, Ordering};

    /// Keep the global ref alive for the process lifetime.
    /// Stores the raw jobject returned by NewGlobalRef.
    static GLOBAL_CTX: AtomicPtr<jni::sys::_jobject> = AtomicPtr::new(std::ptr::null_mut());

    fn logcat(msg: &str) {
        use std::ffi::CString;
        let tag = CString::new("BlewPlugin").unwrap();
        let msg = CString::new(msg).unwrap();
        unsafe {
            ndk_sys::__android_log_write(
                ndk_sys::android_LogPriority::ANDROID_LOG_INFO.0 as _,
                tag.as_ptr(),
                msg.as_ptr(),
            );
        }
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_com_offbeat_offbeat_1mobile_BlewPlugin_init_1android(
        env: *mut jni::sys::JNIEnv,
        _class: jni::sys::jobject,
        ctx: jni::sys::jobject,
    ) {
        logcat("init_android: entered");

        unsafe {
            let interface = *env;

            // Create a global ref so the context outlives this JNI call.
            let global_ctx = ((*interface).v1_1.NewGlobalRef)(env, ctx);
            if global_ctx.is_null() {
                logcat("init_android: ERROR - NewGlobalRef returned null");
                return;
            }
            GLOBAL_CTX.store(global_ctx, Ordering::Release);

            // Get the JavaVM pointer.
            let mut vm_ptr: *mut jni::sys::JavaVM = std::ptr::null_mut();
            let rc = ((*interface).v1_1.GetJavaVM)(env, &mut vm_ptr);
            if rc != 0 || vm_ptr.is_null() {
                logcat("init_android: ERROR - GetJavaVM failed");
                return;
            }

            // Set up ndk_context (required by blew's init_jvm).
            ndk_context::initialize_android_context(
                vm_ptr as *mut c_void,
                global_ctx as *mut c_void,
            );

            // Cache class refs and store the JVM in blew's statics.
            let vm = jni::JavaVM::from_raw(vm_ptr);
            blew::platform::android::init_jvm(vm);

            let perms = blew::platform::android::are_ble_permissions_granted();
            logcat(&format!("init_android: done, perms={perms}"));
        }
    }
}
