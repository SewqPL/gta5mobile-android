package dev.muguet;

/**
 * Retry policy for attaching the touch HUD as a sub-window above the game's
 * native surface. Free of Android types so it can be unit tested on the JVM.
 */
final class HudAttachPolicy {
    private HudAttachPolicy() {}

    /** The window type to try after the given one was rejected. */
    static int nextType(int current, int count) { return (current + 1) % count; }

    /** Delay before the next attach attempt; fast at first, then steady. */
    static long delayMs(int attempt) { return attempt < 10 ? 200 : 1000; }

    /** Attempts worth reporting in the log; after that only every twentieth. */
    static boolean logAttempt(int attempt) { return attempt <= 4 || attempt % 20 == 0; }
}
