package dev.muguet;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.Service;
import android.content.Intent;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;
import android.os.PowerManager;
import java.io.InterruptedIOException;
import java.util.concurrent.CopyOnWriteArraySet;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.atomic.AtomicBoolean;

public final class GameDownloadService extends Service {
    static final String START = "dev.muguet.DOWNLOAD", PAUSE = "dev.muguet.PAUSE_DOWNLOAD";
    private static final String CHANNEL = "game-download";
    private static final int NOTIFICATION = 2;
    interface Observer { void changed(State state); }
    static final class State {
        final boolean running;
        final String text;
        final long done, total;
        State(boolean running, String text, long done, long total) {
            this.running = running; this.text = text; this.done = done; this.total = total;
        }
    }
    private static final CopyOnWriteArraySet<Observer> observers = new CopyOnWriteArraySet<>();
    private static volatile State state = new State(false, "", 0, 0);
    private static final AtomicBoolean working = new AtomicBoolean();
    private final Handler main = new Handler(Looper.getMainLooper());
    private final ExecutorService worker = Executors.newSingleThreadExecutor();
    private GameInstaller installer;
    private PowerManager.WakeLock wakeLock;
    private boolean stopped;
    static State current() { return state; }
    static boolean busy() { return working.get(); }
    static boolean reserve() { return working.compareAndSet(false, true); }
    static void release() { working.set(false); }
    static void localFinished() {
        release(); state = new State(false, "", 0, 0);
        for (Observer observer : observers) observer.changed(state);
    }
    static void observe(Observer observer) { observers.add(observer); observer.changed(state); }
    static void unobserve(Observer observer) { observers.remove(observer); }
    private void publish(boolean running, String text, long done, long total) {
        State next = new State(running, text, done, total); state = next;
        for (Observer observer : observers) observer.changed(next);
        if (running && !stopped) getSystemService(NotificationManager.class).notify(NOTIFICATION, notification(next));
    }
    private Notification notification(State value) {
        PendingIntent launcher = PendingIntent.getActivity(this, 0, new Intent(this, LauncherActivity.class).addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP | Intent.FLAG_ACTIVITY_SINGLE_TOP), PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT);
        PendingIntent pause = PendingIntent.getService(this, 1, new Intent(this, GameDownloadService.class).setAction(PAUSE), PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT);
        Notification.Builder notification = new Notification.Builder(this, CHANNEL)
            .setSmallIcon(android.R.drawable.stat_sys_download).setContentTitle("GTA 5 game installation")
            .setContentText(value.text).setContentIntent(launcher).setOnlyAlertOnce(true).setOngoing(value.running);
        if (value.running) notification.addAction(new Notification.Action.Builder(null, "Pause", pause).build());
        notification.setProgress(1000, value.total <= 0 ? 0 : (int) Math.min(1000, 1000.0 * value.done / value.total), value.running && value.total <= 0);
        return notification.build();
    }
    @Override public void onCreate() {
        super.onCreate();
        getSystemService(NotificationManager.class).createNotificationChannel(new NotificationChannel(CHANNEL, "Game downloads", NotificationManager.IMPORTANCE_LOW));
    }
    @Override public int onStartCommand(Intent intent, int flags, int startId) {
        if (intent != null && PAUSE.equals(intent.getAction())) {
            if (installer != null) { publish(true, "Pausing—saving progress…", 0, 0); installer.cancel(); }
            else stopSelf();
            return START_NOT_STICKY;
        }
        if (installer != null) return START_NOT_STICKY;
        State initial = new State(true, "Starting game download…", 0, 0);
        try { startForeground(NOTIFICATION, notification(initial)); }
        catch (Exception error) {
            if (!busy()) publish(false, "Android could not start the download service: " + error.getMessage(), 0, 0);
            stopSelf(); return START_NOT_STICKY;
        }
        // A timed-out/destroyed service's cancelled worker may still be saving its journal.
        if (!reserve()) { stopForeground(STOP_FOREGROUND_REMOVE); stopSelf(); return START_NOT_STICKY; }
        publish(true, initial.text, 0, 0);
        try {
            wakeLock = getSystemService(PowerManager.class).newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "Muguet:GameDownload");
            wakeLock.acquire(6 * 60 * 60 * 1000L);
            installer = new GameInstaller(getFilesDir(), (text, done, total) -> main.post(() -> { if (!stopped && installer != null) publish(true, text, done, total); }));
        } catch (Exception error) {
            release(); publish(false, "Could not start download: " + error.getMessage(), 0, 0);
            stopForeground(STOP_FOREGROUND_REMOVE); stopSelf(); return START_NOT_STICKY;
        }
        GameInstaller task = installer;
        worker.execute(() -> {
            String result;
            try { task.install(); result = "Game data ready. Download verified and installed."; }
            catch (InterruptedIOException error) { result = "Download paused. Tap Download / Resume to continue."; }
            catch (Exception error) { result = "Download stopped: " + error.getMessage() + " Tap Download / Resume to retry."; }
            String message = result;
            main.post(() -> {
                installer = null;
                if (wakeLock != null && wakeLock.isHeld()) wakeLock.release();
                release();
                publish(false, message, 0, 0);
                if (!stopped) { stopForeground(STOP_FOREGROUND_REMOVE); stopSelf(); }
            });
        });
        return START_NOT_STICKY;
    }
    // Android 15+ limits dataSync foreground services to six background hours per day.
    @Override public void onTimeout(int startId, int foregroundServiceType) {
        stopped = true;
        if (installer != null) installer.cancel();
        publish(true, "Android paused the long download; saving progress…", 0, 0);
        stopForeground(STOP_FOREGROUND_REMOVE); stopSelf();
    }
    @Override public void onDestroy() {
        stopped = true;
        if (installer != null) installer.cancel();
        if (wakeLock != null && wakeLock.isHeld()) wakeLock.release();
        worker.shutdown();
        super.onDestroy();
    }
    @Override public IBinder onBind(Intent intent) { return null; }
}
