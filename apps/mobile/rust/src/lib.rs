pub mod api;
mod frb_generated;

/// Initialize blew's Android JNI layer.
///
/// Called from Kotlin `BlewPlugin.onAttachedToEngine` which passes
/// `applicationContext`. Follows the flutter_rust_bridge "Method 2"
/// pattern for NDK context init (see ndk-init.md in FRB docs),
/// adapted for jni 0.22 API.
#[cfg(target_os = "android")]
mod init_android {
    use jni::objects::{Global, JClass, JObject};
    use std::ffi::c_void;
    use std::sync::OnceLock;

    /// Keep the context Global alive for the process lifetime.
    static CTX: OnceLock<Global<JObject<'static>>> = OnceLock::new();

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_com_offbeat_offbeat_1mobile_BlewPlugin_init_1android<'local>(
        mut env: jni::EnvUnowned<'local>,
        _class: JClass<'local>,
        ctx: JObject<'local>,
    ) {
        env.with_env(|env| {
            let global = env.new_global_ref(&ctx)?;
            let vm = env.get_java_vm()?;
            let vm_ptr = vm.get_raw() as *mut c_void;

            unsafe {
                ndk_context::initialize_android_context(vm_ptr, global.as_obj().as_raw() as _);
            }

            CTX.get_or_init(|| global);

            blew::platform::android::init_jvm(vm);
            Ok::<_, jni::errors::Error>(())
        });
    }
}
