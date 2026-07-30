use std::env;

fn main() {
    // `mobile` — Android and OpenHarmony share a phone-shaped shell: a native
    // composer overlay instead of a docked one, a soft keyboard, a sandboxed
    // per-app HOME, and no desktop window chrome. Gate that shared behaviour on
    // `mobile` rather than repeating
    // `any(target_os = "android", target_env = "ohos")` at every site.
    //
    // NOTE iOS is deliberately NOT included: it has its own shell and its own
    // backend, and folding it in here would silently change its behaviour at
    // every one of these sites. Add it only with per-site review.
    println!("cargo:rustc-check-cfg=cfg(mobile)");
    if env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "android"
        || env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default() == "ohos"
    {
        println!("cargo:rustc-cfg=mobile");
    }
}
