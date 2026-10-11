package dev.muguet;

import android.app.Activity;
import android.media.AudioAttributes;
import android.media.AudioFormat;
import android.media.AudioManager;
import android.media.AudioTrack;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.SurfaceHolder;
import android.view.SurfaceView;
import android.view.View;
import android.view.ViewGroup;
import android.view.WindowManager;
import android.widget.FrameLayout;
import android.widget.TextView;
import java.io.File;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.HashSet;
import org.json.JSONObject;

// Hosts the vendored wasm2c engine core. Unlike GameActivity (which uses
// NativeActivity + winit for the Rust/wasmtime runtime), this uses a plain
// SurfaceView so we own the ANativeWindow handed to the Vulkan renderer, and the
// touch HUD sits directly on top as ordinary child views - no sub-window hack.
//
// The engine boots on its own native thread inside nativeStart; this activity
// feeds it input, audio and the surface, and mirrors status text. With the
// headless graphics stub it will boot and run its non-graphics subsystems but
// not render; that is the expected state until the Vulkan renderer lands.
public final class EngineActivity extends Activity {
    static {
        System.loadLibrary("gta5engine");
        nativeRegister();
    }

    // JNI (registered natively via RegisterNatives).
    private static native int nativeRegister();
    private static native boolean nativeCacheActivity(Object activity);
    private static native void nativeSetHeapMb(int mb);
    private static native void nativeStart(String gameDir, String manifestPath, int startMode,
                                           int width, int height, int frameLimit);
    private static native void nativeAttachSurface(android.view.Surface surface);
    private static native void nativeUpdatePad(String[] keys, int touchButtons, boolean horn,
                                               float lookDX, float lookDY, boolean analog);
    private static native void nativeUpdatePadAnalog(float lx, float ly, float rx, float ry,
                                                     float lt, float rt, int buttons);
    private static native int nativeAudio(short[] out, int maxFrames);
    private static native boolean nativePauseMenuActive();
    private static native int nativeLastInputDevice();
    private static native int nativeTouchContext();
    private static native int nativeFps();

    private final Handler handler = new Handler(Looper.getMainLooper());
    private SurfaceView surface;
    private TouchControls controls;
    private TextView status, fpsText;
    private InputManagerCompat devices;
    private AudioTrack audio;
    private Thread audioThread;
    private volatile boolean running, muted, surfaceReady;
    private boolean focused, controller, inCar;
    private int frameLimit = 30;
    private float lx, ly, rx, ry, lt, rt;
    private final HashSet<String> keyboardKeys = new HashSet<>();
    private final HashSet<Integer> padKeys = new HashSet<>();
    private float accLookX, accLookY;
    private long fpsAt, fpsFrames, lastFps;
    private String statusMessage = "Starting engine…";

    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        getWindow().getDecorView().setSystemUiVisibility(
            View.SYSTEM_UI_FLAG_FULLSCREEN | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION |
            View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN |
            View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION | View.SYSTEM_UI_FLAG_LAYOUT_STABLE);
        getWindow().getAttributes().layoutInDisplayCutoutMode =
            WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;

        FrameLayout root = new FrameLayout(this);
        surface = new SurfaceView(this);
        surface.getHolder().addCallback(new SurfaceHolder.Callback() {
            @Override public void surfaceCreated(SurfaceHolder h) {
                nativeAttachSurface(h.getSurface()); surfaceReady = true;
            }
            @Override public void surfaceChanged(SurfaceHolder h, int f, int w, int ht) {
                // The engine reads its back-buffer size at boot; after that the
                // renderer owns resize. Until it exists, keep the reported size in
                // step so a future renderer picks the right extent.
            }
            @Override public void surfaceDestroyed(SurfaceHolder h) {
                surfaceReady = false; nativeAttachSurface(null);
            }
        });
        root.addView(surface, new FrameLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));

        controls = new TouchControls(this, LookSensitivity.fromConfig(new File(getFilesDir(), "config.json")));
        controls.setVisibility(View.VISIBLE);
        root.addView(controls, new FrameLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));

        status = new TextView(this);
        status.setTextColor(0xffffffff); status.setBackgroundColor(0xb0000000);
        status.setTextSize(16); status.setPadding(16, 8, 16, 8); status.setText(statusMessage);
        root.addView(status, new FrameLayout.LayoutParams(
            ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT, android.view.Gravity.CENTER));

        fpsText = new TextView(this);
        fpsText.setTextColor(0xccffffff); fpsText.setTextSize(12);
        fpsText.setPadding(8, 8, 8, 8); fpsText.setShadowLayer(4, 0, 0, 0xff000000);
        root.addView(fpsText, new FrameLayout.LayoutParams(
            ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT,
            android.view.Gravity.TOP | android.view.Gravity.CENTER_HORIZONTAL));

        devices = new InputManagerCompat(this, new InputManagerCompat.Listener() {
            @Override public void controllersChanged(boolean connected) {
                controller = connected;
                if (connected) controls.setVisibility(View.GONE);
                else controls.setVisibility(View.VISIBLE);
                clearInput();
            }
        });
        setContentView(root);

        startEngine();
    }

    private void startEngine() {
        try {
            nativeCacheActivity(this);
            JSONObject config = readConfig();
            muted = config.optBoolean("mute", false);
            frameLimit = config.optInt("fps_limit", 30) <= 30 ? 30 : 60;
            int startMode = config.optInt("start_mode", 2);
            // EngineHost reads PG_ENGINE_HEAP_MB before boot; set it from the
            // launcher's chosen heap so the process is not killed for committing
            // more than Android's largeHeap allows.
            int heapGb = Math.max(1, Math.min(8, config.optInt("android_heap_gb", 5)));
            nativeSetHeapMb(heapGb * 1024);

            File gameDir = new File(getFilesDir(), "game");
            File manifest = new File(gameDir, "data/manifest.json");
            if (!manifest.isFile()) {
                setStatus("Game files missing. Return to the launcher and install the game first.");
                return;
            }
            int[] size = displaySize();
            int width = Math.max(320, surface.getWidth() > 0 ? surface.getWidth() : size[0]);
            int height = Math.max(180, surface.getHeight() > 0 ? surface.getHeight() : size[1]);
            nativeStart(gameDir.getAbsolutePath(), manifest.getAbsolutePath(), startMode,
                        width, height, frameLimit);
            if (!muted) startAudio();
            running = true;
            handler.post(tick);
        } catch (Exception e) {
            setStatus("Startup failed: " + e.getMessage());
        }
    }

    // The engine reads its back-buffer size at boot. SurfaceView has no size yet
    // in onCreate, so fall back to the real display metrics.
    private int[] displaySize() {
        android.util.DisplayMetrics m = new android.util.DisplayMetrics();
        getWindowManager().getDefaultDisplay().getRealMetrics(m);
        return new int[] { m.widthPixels, m.heightPixels };
    }

    private JSONObject readConfig() {
        try { return new JSONObject(new String(Files.readAllBytes(
                new File(getFilesDir(), "config.json").toPath()), StandardCharsets.UTF_8)); }
        catch (Exception ignored) { return new JSONObject(); }
    }

    // Called from the native engine thread via the log bridge. Never touch views
    // directly here; setStatus marshals to the UI thread itself.
    public void setStatus(final String text) {
        statusMessage = text;
        runOnUiThread(() -> {
            if (status == null) return;
            status.setText(text);
            if (!text.isEmpty()) status.setVisibility(View.VISIBLE);
        });
    }

    // --- audio ---------------------------------------------------------------
    private void startAudio() {
        int rate = 48000, channelConfig = AudioFormat.CHANNEL_OUT_STEREO,
            encoding = AudioFormat.ENCODING_PCM_16BIT;
        int min = AudioTrack.getMinBufferSize(rate, channelConfig, encoding);
        int bufferBytes = Math.max(min, 4096 * 4);
        audio = new AudioTrack.Builder()
            .setAudioAttributes(new AudioAttributes.Builder()
                .setUsage(AudioAttributes.USAGE_GAME).setContentType(AudioAttributes.CONTENT_TYPE_MOVIE).build())
            .setAudioFormat(new AudioFormat.Builder()
                .setEncoding(encoding).setSampleRate(rate).setChannelMask(channelConfig).build())
            .setBufferSizeInBytes(bufferBytes).setTransferMode(AudioTrack.MODE_STREAM).build();
        audio.play();
        audioThread = new Thread(() -> {
            short[] block = new short[4096];
            while (running) {
                int frames = 0;
                try { frames = nativeAudio(block, block.length / 2); } catch (Exception ignored) {}
                if (frames > 0) audio.write(block, 0, frames * 2);
                else { try { Thread.sleep(8); } catch (InterruptedException e) { return; } }
            }
        }, "engine-audio");
        audioThread.setPriority(Thread.MAX_PRIORITY);
        audioThread.start();
    }

    // --- input ---------------------------------------------------------------
    private void clearInput() {
        keyboardKeys.clear(); padKeys.clear();
        lx = ly = rx = ry = lt = rt = 0; accLookX = accLookY = 0;
        if (controls != null) controls.release();
        nativeUpdatePad(new String[0], 0, false, 0, 0, false);
    }

    private final Runnable tick = new Runnable() {
        @Override public void run() {
            if (!running) return;
            // 60 Hz input pump; independent of the engine's frame rate.
            TouchControls.Input touch = controls != null ? controls.input() : new TouchControls.Input();

            // Switch the HUD between on-foot and in-car layouts, as GameActivity did.
            int context = nativeTouchContext();
            boolean nowInCar = context != 0;
            // `this` inside this anonymous Runnable is the Runnable, so the outer
            // field must be reached explicitly.
            if (nowInCar != EngineActivity.this.inCar) {
                EngineActivity.this.inCar = nowInCar;
                if (controls != null) controls.setInCar(nowInCar);
            }

            HashSet<String> keys = new HashSet<>(touch.keys);
            keys.addAll(keyboardKeys);
            int touchButtons = touch.buttons;
            boolean horn = touch.horn;

            if (controller) {
                // A real gamepad keeps its analog sticks; this is what preserves
                // GTA's analog driving and assisted aim. Face/shoulder buttons
                // and the d-pad are still mapped to engine pad bits.
                int padButtons = 0;
                if (down(KeyEvent.KEYCODE_BUTTON_A)) padButtons |= 1 << 6;      // A
                if (down(KeyEvent.KEYCODE_BUTTON_B)) padButtons |= 1 << 5;      // B
                if (down(KeyEvent.KEYCODE_BUTTON_X)) padButtons |= 1 << 7;      // X
                if (down(KeyEvent.KEYCODE_BUTTON_Y)) padButtons |= 1 << 4;      // Y
                if (down(KeyEvent.KEYCODE_BUTTON_L1)) padButtons |= 1 << 2;     // L1
                if (down(KeyEvent.KEYCODE_BUTTON_R1)) padButtons |= 1 << 3;     // R1
                if (down(KeyEvent.KEYCODE_BUTTON_SELECT)) padButtons |= 1 << 8; // Select
                if (down(KeyEvent.KEYCODE_BUTTON_START)) padButtons |= 1 << 11; // Start
                if (down(KeyEvent.KEYCODE_DPAD_UP)) padButtons |= 1 << 12;
                if (down(KeyEvent.KEYCODE_DPAD_DOWN)) padButtons |= 1 << 14;
                if (down(KeyEvent.KEYCODE_DPAD_LEFT)) padButtons |= 1 << 15;
                if (down(KeyEvent.KEYCODE_DPAD_RIGHT)) padButtons |= 1 << 13;
                float gas = Math.max(rt, down(KeyEvent.KEYCODE_BUTTON_R2) ? 1f : 0f);
                float brake = Math.max(lt, down(KeyEvent.KEYCODE_BUTTON_L2) ? 1f : 0f);
                nativeUpdatePadAnalog(lx, ly, rx, ry, brake, gas, padButtons);
                accLookX = 0; accLookY = 0;
                updateFps();
                handler.postDelayed(this, 16);
                return;
            }

            // Touch / keyboard: map onto the engine's virtual pad.
            if (lx < -0.3f) keys.add("KeyA"); if (lx > 0.3f) keys.add("KeyD");
            if (ly < -0.3f) keys.add("KeyW"); if (ly > 0.3f) keys.add("KeyS");
            boolean gasTouch = rt > 0.2f || down(KeyEvent.KEYCODE_BUTTON_R2);
            if (gasTouch) touchButtons |= 1;
            if (lt > 0.2f || down(KeyEvent.KEYCODE_BUTTON_L2)) touchButtons |= 4;
            if (down(KeyEvent.KEYCODE_BUTTON_A)) keys.add("Enter");
            if (down(KeyEvent.KEYCODE_BUTTON_B)) { keys.add("KeyR"); keys.add("Backspace"); }
            if (down(KeyEvent.KEYCODE_BUTTON_X)) keys.add("Space");
            if (down(KeyEvent.KEYCODE_BUTTON_Y)) keys.add("KeyF");
            if (down(KeyEvent.KEYCODE_BUTTON_L1)) keys.add("Tab");
            if (down(KeyEvent.KEYCODE_BUTTON_R1)) keys.add("KeyQ");
            if (down(KeyEvent.KEYCODE_BUTTON_START)) keys.add("Escape");
            if (down(KeyEvent.KEYCODE_BUTTON_SELECT)) keys.add("KeyV");
            accLookX += rx * 320; accLookY += ry * 320;

            // Consume the accumulated look delta each pump.
            nativeUpdatePad(keys.toArray(new String[0]), touchButtons, horn,
                            accLookX, accLookY, true);
            accLookX = 0; accLookY = 0;

            updateFps();
            handler.postDelayed(this, 16);
        }
    };

    private boolean down(int key) { return padKeys.contains(key); }
    private static boolean isPad(int source) {
        return (source & android.view.InputDevice.SOURCE_GAMEPAD) == android.view.InputDevice.SOURCE_GAMEPAD
            || (source & android.view.InputDevice.SOURCE_JOYSTICK) == android.view.InputDevice.SOURCE_JOYSTICK;
    }
    @Override public boolean dispatchKeyEvent(KeyEvent event) {
        if (isPad(event.getSource())) {
            int key = event.getKeyCode();
            if (event.getAction() == KeyEvent.ACTION_DOWN) padKeys.add(key); else padKeys.remove(key);
            return true;
        }
        String mapped = keyboardKey(event.getKeyCode());
        if (mapped != null) {
            if (event.getAction() == KeyEvent.ACTION_DOWN) keyboardKeys.add(mapped); else keyboardKeys.remove(mapped);
            return true;
        }
        return super.dispatchKeyEvent(event);
    }
    private static String keyboardKey(int key) {
        if (key >= KeyEvent.KEYCODE_A && key <= KeyEvent.KEYCODE_Z) return "Key" + (char) ('A' + key - KeyEvent.KEYCODE_A);
        if (key >= KeyEvent.KEYCODE_0 && key <= KeyEvent.KEYCODE_9) return "Digit" + (key - KeyEvent.KEYCODE_0);
        switch (key) {
            case KeyEvent.KEYCODE_ENTER: return "Enter";
            case KeyEvent.KEYCODE_ESCAPE: return "Escape";
            case KeyEvent.KEYCODE_DEL: return "Backspace";
            case KeyEvent.KEYCODE_SPACE: return "Space";
            case KeyEvent.KEYCODE_TAB: return "Tab";
            case KeyEvent.KEYCODE_SHIFT_LEFT: return "ShiftLeft";
            case KeyEvent.KEYCODE_CTRL_LEFT: return "ControlLeft";
            case KeyEvent.KEYCODE_ALT_LEFT: return "AltLeft";
            case KeyEvent.KEYCODE_DPAD_UP: return "ArrowUp";
            case KeyEvent.KEYCODE_DPAD_DOWN: return "ArrowDown";
            case KeyEvent.KEYCODE_DPAD_LEFT: return "ArrowLeft";
            case KeyEvent.KEYCODE_DPAD_RIGHT: return "ArrowRight";
            default: return null;
        }
    }
    @Override public boolean dispatchGenericMotionEvent(MotionEvent e) {
        if (isPad(e.getSource())) {
            lx = e.getAxisValue(MotionEvent.AXIS_X); ly = e.getAxisValue(MotionEvent.AXIS_Y);
            rx = e.getAxisValue(MotionEvent.AXIS_Z); ry = e.getAxisValue(MotionEvent.AXIS_RZ);
            lt = Math.max(e.getAxisValue(MotionEvent.AXIS_LTRIGGER), e.getAxisValue(MotionEvent.AXIS_BRAKE));
            rt = Math.max(e.getAxisValue(MotionEvent.AXIS_RTRIGGER), e.getAxisValue(MotionEvent.AXIS_GAS));
            return true;
        }
        return super.dispatchGenericMotionEvent(e);
    }

    private void updateFps() {
        long now = System.nanoTime();
        long frames = nativeFps();
        if (fpsAt == 0) { fpsAt = now; fpsFrames = frames; return; }
        long elapsed = now - fpsAt;
        if (elapsed < 500_000_000L) return;
        float fps = (frames - fpsFrames) * 100.0f / (elapsed / 1e9f);
        fpsAt = now; fpsFrames = frames; lastFps = (long) fps;
        if (fpsText != null) fpsText.setText(String.format(java.util.Locale.ROOT, "%.0f FPS", fps));
    }

    // --- lifecycle -----------------------------------------------------------
    @Override protected void onResume() {
        super.onResume(); running = true; devices.start(); handler.post(tick);
    }
    @Override protected void onPause() {
        running = false; handler.removeCallbacks(tick); devices.stop(); clearInput();
        if (audio != null) { try { audio.pause(); } catch (Exception ignored) {} }
        super.onPause();
    }
    @Override public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        this.focused = hasFocus;
        if (!hasFocus) clearInput();
    }
    @Override protected void onDestroy() {
        running = false;
        handler.removeCallbacksAndMessages(null);
        if (audio != null) { try { audio.stop(); } catch (Exception ignored) {} }
        if (audioThread != null) audioThread.interrupt();
        super.onDestroy();
    }
    @Override public void onBackPressed() {
        // Back leaves the engine; the engine runs in its own process so a later
        // Play starts clean.
        super.onBackPressed();
        finish();
        android.os.Process.killProcess(android.os.Process.myPid());
    }
}