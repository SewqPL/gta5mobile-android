package dev.muguet;

import android.app.NativeActivity;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.view.*;
import android.widget.FrameLayout;
import android.widget.TextView;
import java.util.HashSet;

// NativeActivity hosts winit/Vulkan; Android views provide the touch HUD.
public final class GameActivity extends NativeActivity implements InputManagerCompat.Listener {
    static { System.loadLibrary("muguet"); }
    static native void nativeInput(String[] keys, int buttons, boolean horn, boolean controller);
    static native void nativeLook(float dx, float dy);
    static native boolean nativeInVehicle();
    static native boolean nativeHasFrame();
    private final Handler handler = new Handler(Looper.getMainLooper());
    private final HashSet<Integer> padKeys = new HashSet<>();
    private final HashSet<String> keyboardKeys = new HashSet<>();
    private TouchControls controls;
    private TextView status;
    private InputManagerCompat devices;
    private boolean active, focused, controller, inCar;
    private float lx, ly, rx, ry, lt, rt, hx, hy;
    private long lastTick;
    private String statusMessage = "Loading…";

    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        getWindow().getDecorView().setSystemUiVisibility(View.SYSTEM_UI_FLAG_FULLSCREEN | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION |
            View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION | View.SYSTEM_UI_FLAG_LAYOUT_STABLE);
        getWindow().getAttributes().layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
        FrameLayout overlay = new FrameLayout(this);
        controls = new TouchControls(this);
        overlay.addView(controls, new FrameLayout.LayoutParams(-1, -1));
        status = new TextView(this);
        status.setTextColor(0xffffffff); status.setBackgroundColor(0xb0000000); status.setTextSize(16); status.setPadding(16, 8, 16, 8);
        status.setText(statusMessage);
        FrameLayout.LayoutParams params = new FrameLayout.LayoutParams(-2, -2, Gravity.CENTER);
        overlay.addView(status, params);
        addContentView(overlay, new ViewGroup.LayoutParams(-1, -1));
        devices = new InputManagerCompat(this, this);
    }
    public void setStatus(String text) { runOnUiThread(() -> { statusMessage = text; if (status != null) { status.setText(text); status.setVisibility(View.VISIBLE); } }); }
    @Override protected void onResume() {
        super.onResume(); active = true; lastTick = System.nanoTime(); devices.start(); handler.post(tick);
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
        handler.removeCallbacksAndMessages(null); super.onDestroy();
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
            if (nativeHasFrame() && !statusMessage.startsWith("Startup failed") && !statusMessage.startsWith("Graphics initialization failed")) status.setVisibility(View.GONE);
            handler.postDelayed(this, 16);
        }
    };
}
