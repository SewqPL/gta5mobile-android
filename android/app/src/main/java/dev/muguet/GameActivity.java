package dev.muguet;

import android.app.NativeActivity;
import android.os.Bundle;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;
import android.util.Log;
import android.view.*;
import android.widget.FrameLayout;
import android.widget.TextView;
import android.widget.Toast;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.OutputStream;
import java.nio.charset.StandardCharsets;
import java.util.HashSet;

// NativeActivity hosts winit/Vulkan; Android views provide the touch HUD.
public final class GameActivity extends NativeActivity implements InputManagerCompat.Listener {
    static { System.loadLibrary("muguet"); }
    static native void nativeInput(String[] keys, int buttons, boolean horn, boolean controller);
    static native void nativeLook(float dx, float dy);
    static native boolean nativeInVehicle();
    static native boolean nativeHasFrame();
    static native long nativePresents();
    private final Handler handler = new Handler(Looper.getMainLooper());
    private final HashSet<Integer> padKeys = new HashSet<>();
    private final HashSet<String> keyboardKeys = new HashSet<>();
    private TouchControls controls;
    private TextView status, fpsText;
    private InputManagerCompat devices;
    private boolean active, focused, controller, inCar;
    private float lx, ly, rx, ry, lt, rt, hx, hy;
    private long lastTick;
    private String statusMessage = "Loading…";
    private long fpsAt, fpsFrames;

    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        getWindow().getDecorView().setSystemUiVisibility(View.SYSTEM_UI_FLAG_FULLSCREEN | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION |
            View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION | View.SYSTEM_UI_FLAG_LAYOUT_STABLE);
        getWindow().getAttributes().layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
        controls = new TouchControls(this, LookSensitivity.fromConfig(new File(getFilesDir(), "config.json")));
        status = new TextView(this);
        status.setTextColor(0xffffffff); status.setBackgroundColor(0xb0000000); status.setTextSize(16); status.setPadding(16, 8, 16, 8);
        status.setText(statusMessage);
        fpsText = new TextView(this);
        fpsText.setTextColor(0xccffffff); fpsText.setTextSize(12); fpsText.setPadding(8, 8, 8, 8);
        fpsText.setShadowLayer(4, 0, 0, 0xff000000);
        devices = new InputManagerCompat(this, this);
        // The native Vulkan surface owns the activity window, so HUD views added to
        // the content view sit below it. A panel sub-window composites above the
        // activity window and still forwards keys/touch to the game where the HUD
        // does not consume them. No overlay permission is required for a sub-window.
        attachHud();
    }

    private FrameLayout overlay;
    private boolean hudAttached;
    private boolean hudStale;
    private int hudAttempts;
    private int hudType;
    // Sub-window types that composite above the activity window. The panel type
    // is the documented one; the attached-dialog type is what some vendor
    // builds accept when a panel add is rejected.
    private static final int[] HUD_TYPES = {
        WindowManager.LayoutParams.TYPE_APPLICATION_PANEL,
        WindowManager.LayoutParams.TYPE_APPLICATION_ATTACHED_DIALOG,
    };

    // The HUD needs its own window: NativeActivity hands the activity window's
    // surface to the renderer (getWindow().takeSurface), so the framework never
    // draws the view hierarchy and anything in the content view is invisible.
    // A panel sub-window composites above the game instead. Adding it fails
    // with a BadTokenException while the activity window has no token yet, and
    // the old code hid that failure behind a content-view fallback that can
    // never be seen: the player got gameplay with no touch controls.
    private void attachHud() {
        tryAttachHud();
    }

    // A window manager root can only be added once: a view that was added
    // before keeps its parent, so a retry needs a fresh root.
    private void rebuildOverlay() {
        FrameLayout previous = overlay;
        FrameLayout layer = new FrameLayout(this);
        if (previous != null) {
            previous.removeView(controls);
            previous.removeView(status);
            previous.removeView(fpsText);
        }
        layer.addView(controls, new FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
        layer.addView(status, new FrameLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT, Gravity.CENTER));
        layer.addView(fpsText, new FrameLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT, Gravity.TOP | Gravity.CENTER_HORIZONTAL));
        layer.setBackground(new android.graphics.drawable.ColorDrawable(android.graphics.Color.TRANSPARENT));
        layer.addOnAttachStateChangeListener(new View.OnAttachStateChangeListener() {
            @Override public void onViewAttachedToWindow(View v) {}
            @Override public void onViewDetachedFromWindow(View v) {
                // The system drops the panel when the activity is paused; a
                // stale root that we replaced is not the live one.
                if (v != overlay) return;
                hudAttached = false;
                hudStale = true;
                if (active) scheduleHudAttach("HUD window dropped");
            }
        });
        overlay = layer;
        hudStale = false;
        if (previous != null) {
            try { getSystemService(WindowManager.class).removeView(previous); }
            catch (Throwable ignored) {}
        }
    }

    private void tryAttachHud() {
        if (isFinishing() || isDestroyed() || hudAttached) return;
        if (overlay == null || hudStale) rebuildOverlay();
        IBinder token = getWindow().getDecorView().getWindowToken();
        if (token == null) { scheduleHudAttach("no window token yet"); return; }
        int type = HUD_TYPES[hudType];
        try {
            getSystemService(WindowManager.class).addView(overlay, hudParams(token, type));
            hudAttached = true;
            hudAttempts = 0;
            hudLog("HUD window attached (type " + type + ")");
        } catch (Throwable t) {
            hudType = HudAttachPolicy.nextType(hudType, HUD_TYPES.length);
            hudStale = true;
            scheduleHudAttach("attach failed (type " + type + "): " + t);
        }
    }

    // The panel is dropped when the activity is paused; the detach callback
    // clears the flag, so a resume just needs to try again.
    private void checkHud() { tryAttachHud(); }

    private void scheduleHudAttach(String why) {
        hudAttempts++;
        if (HudAttachPolicy.logAttempt(hudAttempts)) hudLog("HUD attach attempt " + hudAttempts + ": " + why);
        if (hudAttempts == 20) Toast.makeText(this, "Touch controls unavailable", Toast.LENGTH_LONG).show();
        // While the activity is paused the window token is gone; onResume
        // starts the retry loop again.
        if (!active) return;
        handler.postDelayed(this::tryAttachHud, HudAttachPolicy.delayMs(hudAttempts));
    }

    private WindowManager.LayoutParams hudParams(IBinder token, int type) {
        WindowManager.LayoutParams params = new WindowManager.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT,
            type,
            // Not focusable: keyboard and gamepad keep reaching the native activity.
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE | WindowManager.LayoutParams.FLAG_HARDWARE_ACCELERATED,
            android.graphics.PixelFormat.TRANSLUCENT);
        params.gravity = Gravity.TOP | Gravity.START;
        params.token = token;
        return params;
    }

    // The exported log is the only thing a bug reporter can share, so the HUD
    // state goes into the same file the launcher exports.
    private void hudLog(String message) {
        String line = "[hud] " + message;
        Log.i("Muguet", line);
        try (OutputStream out = new FileOutputStream(new File(getFilesDir(), "muguet.log"), true)) {
            out.write((line + "\n").getBytes(StandardCharsets.UTF_8));
        } catch (IOException ignored) {}
    }
    public void setStatus(String text) { runOnUiThread(() -> { statusMessage = text; if (status != null) { status.setText(text); status.setVisibility(View.VISIBLE); } }); }
    @Override protected void onResume() {
        super.onResume(); active = true; lastTick = System.nanoTime(); devices.start(); handler.post(tick);
        // The system drops the panel when the activity is paused and the token
        // changes with it, so check the HUD again once the window settles.
        handler.postDelayed(this::checkHud, 500);
    }
    @Override protected void onPause() {
        active = false; handler.removeCallbacks(tick); devices.stop(); clearInput(); super.onPause();
    }

    @Override public void onWindowFocusChanged(boolean focused) {
        super.onWindowFocusChanged(focused);
        this.focused = focused;
        if (!focused && controls != null) clearInput();
    }
    @Override protected void onDestroy() {
        handler.removeCallbacksAndMessages(null);
        if (overlay != null) {
            try { getSystemService(WindowManager.class).removeView(overlay); }
            catch (Exception ignored) {}
            overlay = null; hudAttached = false;
        }
        super.onDestroy();
        // Runtime workers are process-lifetime; a separate :game process lets
        // the launcher survive and a later Play create a clean Wasmtime runtime.
        android.os.Process.killProcess(android.os.Process.myPid());
    }
    private void clearInput() {
        padKeys.clear(); keyboardKeys.clear(); lx = ly = rx = ry = lt = rt = hx = hy = 0;
        controls.release(); nativeInput(new String[0], 0, false, controller);
    }
    @Override public void controllersChanged(boolean connected) {
        controller = connected; clearInput(); controls.setVisibility(connected ? View.GONE : View.VISIBLE);
        hudLog("controller " + (connected ? "connected" : "disconnected") + ", touch HUD " + (connected ? "hidden" : "shown"));
    }
    private static boolean isPad(int source) { return (source & InputDevice.SOURCE_GAMEPAD) == InputDevice.SOURCE_GAMEPAD || (source & InputDevice.SOURCE_JOYSTICK) == InputDevice.SOURCE_JOYSTICK; }
    @Override public boolean dispatchKeyEvent(KeyEvent event) {
        if (isPad(event.getSource())) {
            int key = event.getKeyCode();
            if (event.getAction() == KeyEvent.ACTION_DOWN) padKeys.add(key);
            else if (event.getAction() == KeyEvent.ACTION_UP) padKeys.remove(key);
            return true;
        }
        String mapped = keyboardKey(event.getKeyCode());
        if (mapped != null) {
            if (event.getAction() == KeyEvent.ACTION_DOWN) keyboardKeys.add(mapped);
            else if (event.getAction() == KeyEvent.ACTION_UP) keyboardKeys.remove(mapped);
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
            hx = e.getAxisValue(MotionEvent.AXIS_HAT_X); hy = e.getAxisValue(MotionEvent.AXIS_HAT_Y);
            return true;
        }
        return super.dispatchGenericMotionEvent(e);
    }
    private boolean down(int key) { return padKeys.contains(key); }
    private static float curve(float v) { float a = Math.abs(v); return a < 0.12f ? 0 : (float) Math.copySign(Math.pow((a - 0.12) / 0.88, 1.8), v); }
    private final Runnable tick = new Runnable() {
        @Override public void run() {
            if (!active) return;
            if (!focused) { handler.postDelayed(this, 16); return; }
            long now = System.nanoTime(); float dt = Math.min(0.05f, (now - lastTick) / 1e9f); lastTick = now;
            boolean car = nativeInVehicle();
            if (car != inCar) { inCar = car; controls.setInCar(car); }
            TouchControls.Input touch = controls.input();
            HashSet<String> keys = new HashSet<>(touch.keys); keys.addAll(keyboardKeys);
            int buttons = touch.buttons;
            boolean horn = touch.horn;
            if (controller) {
                if (lx < -0.3) keys.add("KeyA"); if (lx > 0.3) keys.add("KeyD");
                if (ly < -0.3) keys.add(inCar ? "Numpad8" : "KeyW"); if (ly > 0.3) keys.add(inCar ? "Numpad2" : "KeyS");
                boolean gas = rt > 0.2 || down(KeyEvent.KEYCODE_BUTTON_R2), brake = lt > 0.2 || down(KeyEvent.KEYCODE_BUTTON_L2);
                if (inCar) { if (gas) keys.add("KeyW"); if (brake) keys.add("KeyS"); horn |= down(KeyEvent.KEYCODE_BUTTON_THUMBL); }
                else {
                    if (gas) buttons |= 1; if (brake) buttons |= 2;
                    if (down(KeyEvent.KEYCODE_BUTTON_A)) keys.add("ShiftLeft");
                    if (down(KeyEvent.KEYCODE_BUTTON_THUMBL)) keys.add("ControlLeft");
                }
                if (down(KeyEvent.KEYCODE_BUTTON_A)) keys.add("Enter");
                if (down(KeyEvent.KEYCODE_BUTTON_B)) { keys.add("KeyR"); keys.add("Backspace"); }
                if (down(KeyEvent.KEYCODE_BUTTON_X)) keys.add("Space");
                if (down(KeyEvent.KEYCODE_BUTTON_Y)) keys.add("KeyF");
                if (down(KeyEvent.KEYCODE_BUTTON_L1)) keys.add("Tab");
                if (down(KeyEvent.KEYCODE_BUTTON_R1)) keys.add(inCar ? "Space" : "KeyQ");
                if (down(KeyEvent.KEYCODE_BUTTON_THUMBR)) keys.add("KeyC");
                if (down(KeyEvent.KEYCODE_BUTTON_START)) keys.add("Escape");
                if (down(KeyEvent.KEYCODE_BUTTON_SELECT)) keys.add("KeyV");
                if (hy < -0.5 || down(KeyEvent.KEYCODE_DPAD_UP)) keys.add("ArrowUp");
                if (hy > 0.5 || down(KeyEvent.KEYCODE_DPAD_DOWN)) keys.add("ArrowDown");
                if (hx < -0.5 || down(KeyEvent.KEYCODE_DPAD_LEFT)) keys.add("ArrowLeft");
                if (hx > 0.5 || down(KeyEvent.KEYCODE_DPAD_RIGHT)) keys.add("ArrowRight");
                nativeLook(curve(rx) * 700 * dt, curve(ry) * 700 * dt);
            }
            nativeInput(keys.toArray(new String[0]), buttons, horn, controller);
            updateFps(now);
            if (nativeHasFrame() && !statusMessage.startsWith("Startup failed") && !statusMessage.startsWith("Graphics initialization failed") && !statusMessage.startsWith("Graphics device lost")) status.setVisibility(View.GONE);
            handler.postDelayed(this, 16);
        }
    };

    // Sampled from the native present counter so the readout reflects real frames.
    private void updateFps(long now) {
        long presents = nativePresents();
        if (fpsAt == 0) { fpsAt = now; fpsFrames = presents; return; }
        long elapsed = now - fpsAt;
        if (elapsed < 500_000_000L) return;
        float fps = (presents - fpsFrames) * 1e9f / elapsed;
        fpsAt = now; fpsFrames = presents;
        if (fpsText != null) fpsText.setText(String.format(java.util.Locale.ROOT, "%.0f FPS", fps));
    }
}
