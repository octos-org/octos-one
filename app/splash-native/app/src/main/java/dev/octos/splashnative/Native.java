package dev.octos.splashnative;

import java.nio.ByteBuffer;

/**
 * The JNI surface.
 *
 * Rust owns the Splash VM (ymote/Splash) and the encoded buffer; Java owns every View.
 * No {@code jobject} crosses back into Rust, so ART's 512-local-reference abort and the
 * {@code FindClass} classloader trap are unreachable by construction rather than by
 * care.
 */
public final class Native {
    static { System.loadLibrary("octos_splash_native"); }

    /**
     * Evaluate a Splash card and return its node tree as a direct buffer.
     *
     * Never null for a bad card: it returns a tree describing the failure, because a
     * blank screen cannot be told apart from a crash.
     */
    public static native ByteBuffer renderCard(String splashSource);

    /** Why the last render looks the way it does. */
    public static native String diag();

    /** Which {@code sys.*} helpers this backend implements. */
    public static native String capabilities();

    /** Request/cache-hit counts for the last render. */
    public static native String fetchStats();
}
