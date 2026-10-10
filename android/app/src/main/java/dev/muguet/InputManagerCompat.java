package dev.muguet;

import android.content.Context;
import android.hardware.input.InputManager;
import android.view.InputDevice;

// Keep controller disconnects from leaving simulated keys pressed.
final class InputManagerCompat implements InputManager.InputDeviceListener {
    interface Listener { void controllersChanged(boolean connected); }
    private final InputManager manager;
    private final Listener listener;
    InputManagerCompat(Context context, Listener listener) {
        manager = (InputManager) context.getSystemService(Context.INPUT_SERVICE); this.listener = listener;
    }
    void start() { manager.registerInputDeviceListener(this, null); update(); }
    void stop() { manager.unregisterInputDeviceListener(this); }
    private void update() {
        boolean connected = false;
        for (int id : manager.getInputDeviceIds()) {
            InputDevice device = manager.getInputDevice(id);
            if (device != null && !device.isVirtual() && (device.supportsSource(InputDevice.SOURCE_GAMEPAD) || device.supportsSource(InputDevice.SOURCE_JOYSTICK))) connected = true;
        }
        listener.controllersChanged(connected);
    }
    @Override public void onInputDeviceAdded(int id) { update(); }
    @Override public void onInputDeviceRemoved(int id) { update(); }
    @Override public void onInputDeviceChanged(int id) { update(); }
}
