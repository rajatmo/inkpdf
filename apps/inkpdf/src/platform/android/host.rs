//! The Java side of the app, called through JNI: the file picker and the files it hands out (the Storage Access Framework), the system
//! clipboard and opening links. The Java is `tools/android/java/app/inkpdf/test/Host.java`, a subclass of `NativeActivity` that the manifest names
//! as the activity; this module calls its public methods, and everything above it sees only the [`Bridge`] (so it is tested without a phone).
//! Every call attaches the calling thread to the Java VM (once for good) and turns a Java exception into an error.

use jni::objects::{GlobalRef, JByteArray, JObject, JString, JValue};
use jni::{JNIEnv, JavaVM};

use super::super::Pick;
use super::super::saf::Bridge;

pub struct Host {
    vm: JavaVM,
    activity: GlobalRef,
}

impl Host {
    /// The bridge to the activity of `app`, if its Java VM and activity can be had.
    pub fn new(vm: *mut std::ffi::c_void, activity: *mut std::ffi::c_void) -> Option<Host> {
        // SAFETY: both pointers are the ones the activity glue holds for the life of the app: the Java VM, and the activity object (a
        // global reference it keeps). The VM is only used to attach threads; the activity is copied into a global reference of ours.
        let vm = unsafe { JavaVM::from_raw(vm.cast()) }.ok()?;
        let activity = {
            let env = vm.attach_current_thread_permanently().ok()?;
            // SAFETY: as above; the local view is dropped without freeing it (`JObject` does not own it).
            let local = unsafe { JObject::from_raw(activity.cast()) };
            env.new_global_ref(&local).ok()?
        };
        Some(Host { vm, activity })
    }

    fn env(&self) -> Result<JNIEnv<'_>, String> {
        self.vm.attach_current_thread_permanently().map_err(|e| format!("the Java side is not there: {e}"))
    }

    /// Call `name` on the activity, clearing (and wording) any exception it threw.
    fn call<'a>(&self, env: &mut JNIEnv<'a>, name: &str, signature: &str, args: &[JValue]) -> Result<jni::objects::JValueOwned<'a>, String> {
        let result = env.call_method(&self.activity, name, signature, args);
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear();
            return Err(format!("{name} threw"));
        }
        result.map_err(|e| format!("{name}: {e}"))
    }

    fn string_result(&self, name: &str, signature: &str, args: &[JValue]) -> Option<String> {
        let mut env = self.env().ok()?;
        let object = self.call(&mut env, name, signature, args).ok()?.l().ok()?;
        if object.is_null() {
            return None;
        }
        let text: String = env.get_string(&JString::from(object)).ok()?.into();
        Some(text)
    }

    fn last_error(&self) -> String {
        self.string_result("lastError", "()Ljava/lang/String;", &[])
            .filter(|e| !e.is_empty())
            .unwrap_or_else(|| "the system could not do it".to_string())
    }
}

impl Bridge for Host {
    fn read(&self, uri: &str) -> Result<Vec<u8>, String> {
        let mut env = self.env()?;
        let uri = env.new_string(uri).map_err(|e| e.to_string())?;
        let object = self.call(&mut env, "read", "(Ljava/lang/String;)[B", &[JValue::Object(&uri)])?.l().map_err(|e| e.to_string())?;
        if object.is_null() {
            return Err(self.last_error());
        }
        env.convert_byte_array(JByteArray::from(object)).map_err(|e| e.to_string())
    }

    fn write(&self, uri: &str, bytes: &[u8]) -> Result<(), String> {
        let mut env = self.env()?;
        let uri = env.new_string(uri).map_err(|e| e.to_string())?;
        let data = env.byte_array_from_slice(bytes).map_err(|e| e.to_string())?;
        let answer = self
            .call(&mut env, "write", "(Ljava/lang/String;[B)Ljava/lang/String;", &[JValue::Object(&uri), JValue::Object(&data)])?
            .l()
            .map_err(|e| e.to_string())?;
        if answer.is_null() {
            return Ok(());
        }
        let why: String = env.get_string(&JString::from(answer)).map(Into::into).unwrap_or_else(|_| "the system refused".to_string());
        Err(why)
    }

    fn copy(&self, text: &str) {
        if let Ok(mut env) = self.env()
            && let Ok(text) = env.new_string(text)
        {
            let _ = self.call(&mut env, "copy", "(Ljava/lang/String;)V", &[JValue::Object(&text)]);
        }
    }

    fn paste(&self) -> Option<String> {
        self.string_result("paste", "()Ljava/lang/String;", &[])
    }

    fn open_url(&self, url: &str) {
        if let Ok(mut env) = self.env()
            && let Ok(url) = env.new_string(url)
            && let Err(why) = self.call(&mut env, "openUrl", "(Ljava/lang/String;)V", &[JValue::Object(&url)])
        {
            eprintln!("could not hand the link to the system: {why}");
        }
    }

    fn set_bars_hidden(&self, hidden: bool) {
        if let Ok(mut env) = self.env()
            && let Err(why) = self.call(&mut env, "setBars", "(Z)V", &[JValue::Bool(hidden.into())])
        {
            eprintln!("could not show or hide the system bars: {why}");
        }
    }

    fn pick(&self, kind: Pick, name: &str) {
        if let Ok(mut env) = self.env()
            && let Ok(name) = env.new_string(name)
        {
            let kind = match kind {
                Pick::Open => 0,
                Pick::Save => 1,
            };
            let _ = self.call(&mut env, "pick", "(ILjava/lang/String;)V", &[JValue::Int(kind), JValue::Object(&name)]);
        }
    }

    fn take_picked(&self) -> Option<String> {
        self.string_result("takePicked", "()Ljava/lang/String;", &[])
    }

    fn take_launch(&self) -> Option<String> {
        self.string_result("takeLaunch", "()Ljava/lang/String;", &[])
    }
}
