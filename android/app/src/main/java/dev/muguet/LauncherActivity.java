package dev.muguet;

import android.app.Activity;
import android.app.AlertDialog;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.view.View;
import android.widget.*;
import org.json.JSONObject;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

public final class LauncherActivity extends Activity {
    static { System.loadLibrary("muguet"); }
    private static final int PICK_ZIP = 1;
    private static native boolean nativePrepare(String directory);
    private native void nativeImport(String archive);
    private final ExecutorService worker = Executors.newSingleThreadExecutor();
    private TextView status;
    private Button play, importButton, downloadButton, pauseButton, discardButton;
    private ProgressBar progress;
    private Spinner mode, fps, heap;
    private EditText scale, server, name;
    private CheckBox mute, multiplayer, host;
    private JSONObject settings;
    private boolean importing;
    private final GameDownloadService.Observer downloadObserver = state -> runOnUiThread(() -> downloadState(state));

    @Override public void onCreate(Bundle saved) {
        super.onCreate(saved);
        ScrollView scroll = new ScrollView(this);
        LinearLayout column = new LinearLayout(this);
        column.setOrientation(LinearLayout.VERTICAL);
        int padding = (int) (24 * getResources().getDisplayMetrics().density);
        column.setPadding(padding, padding, padding, padding);
        scroll.addView(column);
        setContentView(scroll);
        TextView title = new TextView(this);
        title.setText("GTA 5 — Android (experimental)");
        title.setTextSize(24);
        column.addView(title);
        TextView warning = new TextView(this);
        warning.setText("ARM64 / Android 10+ / Vulkan. A high-end phone with 8–12 GB RAM is recommended, not a guarantee of compatibility. Download the web-port files or import your own compatible ZIP; a normal PC installation is not enough. First launch compiles the game and may take several minutes.");
        column.addView(warning);
        status = new TextView(this);
        column.addView(status);
        progress = new ProgressBar(this, null, android.R.attr.progressBarStyleHorizontal);
        progress.setMax(1000);
        column.addView(progress);
        downloadButton = new Button(this);
        downloadButton.setText("Download / Resume Game");
        downloadButton.setOnClickListener(v -> confirmDownload());
        column.addView(downloadButton);
        pauseButton = new Button(this);
        pauseButton.setText("Pause download");
        pauseButton.setOnClickListener(v -> startService(new Intent(this, GameDownloadService.class).setAction(GameDownloadService.PAUSE)));
        column.addView(pauseButton);
        discardButton = new Button(this);
        discardButton.setText("Discard partial download");
        discardButton.setOnClickListener(v -> new AlertDialog.Builder(this).setTitle("Discard partial download?")
            .setMessage("Delete only unfinished download files. Your installed game and saves are kept.")
            .setNegativeButton("Keep", null).setPositiveButton("Discard", (dialog, which) -> discardDownload()).show());
        column.addView(discardButton);
        importButton = new Button(this);
        importButton.setText("Import game ZIP");
        importButton.setOnClickListener(v -> {
            Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
            intent.addCategory(Intent.CATEGORY_OPENABLE);
            intent.setType("*/*");
            startActivityForResult(intent, PICK_ZIP);
        });
        column.addView(importButton);
        try { settings = new JSONObject(new String(Files.readAllBytes(config().toPath()), StandardCharsets.UTF_8)); }
        catch (Exception ignored) { settings = new JSONObject(); }
        mode = choice(column, "Start mode", new String[]{"Story", "Sandbox"}, settings.optInt("start_mode", 2) == 1 ? 0 : 1);
        fps = choice(column, "Frame limit", new String[]{"30 FPS", "60 FPS"}, settings.optInt("fps_limit", 30) <= 30 ? 0 : 1);
        heap = choice(column, "Maximum game heap (not total app memory)", new String[]{"3 GB", "4 GB", "5 GB", "6 GB", "7 GB", "8 GB"}, Math.max(0, Math.min(5, settings.optInt("android_heap_gb", 5) - 3)));
        scale = field(column, "Render scale (0.25–1.0)", Double.toString(settings.optDouble("scale", 0.5)));
        scale.setInputType(android.text.InputType.TYPE_CLASS_NUMBER | android.text.InputType.TYPE_NUMBER_FLAG_DECIMAL);
        mute = check(column, "Mute audio", settings.optBoolean("mute", false));
        multiplayer = check(column, "Multiplayer (Muguet UDP, not GTA Online)", settings.optBoolean("multiplayer", false));
        host = check(column, "Host a session", settings.optBoolean("host", false));
        server = field(column, "Server address (when joining)", settings.optString("server", ""));
        name = field(column, "Player name", settings.optString("player_name", ""));
        play = new Button(this);
        play.setText("Play");
        play.setOnClickListener(v -> {
            try {
                double renderScale = Double.parseDouble(scale.getText().toString());
                if (!Double.isFinite(renderScale) || renderScale < 0.25 || renderScale > 1) throw new IllegalArgumentException("Render scale must be between 0.25 and 1.0");
                settings.put("scale", renderScale);
                settings.put("start_mode", mode.getSelectedItemPosition() + 1);
                settings.put("fps_limit", fps.getSelectedItemPosition() == 0 ? 30 : 60);
                settings.put("android_heap_gb", heap.getSelectedItemPosition() + 3);
                settings.put("texture_quality", settings.optInt("texture_quality", 0));
                settings.put("fullscreen", true);
                settings.put("mute", mute.isChecked());
                settings.put("multiplayer", multiplayer.isChecked());
                settings.put("host", host.isChecked());
                settings.put("server", server.getText().toString().trim());
                settings.put("player_name", name.getText().toString().trim());
                File temp = new File(getFilesDir(), "config.json.new");
                Files.write(temp.toPath(), settings.toString(2).getBytes(StandardCharsets.UTF_8));
                Files.move(temp.toPath(), config().toPath(), java.nio.file.StandardCopyOption.REPLACE_EXISTING);
                startActivity(new Intent(this, GameActivity.class));
            } catch (Exception e) { error(e); }
        });
        column.addView(play);
        ready();
        downloadState(GameDownloadService.current());
    }

    private void confirmDownload() {
        if (importing || GameDownloadService.busy()) return;
        new AlertDialog.Builder(this).setTitle("Download game files?")
            .setMessage("Manifest: gcode.bytarch.com/download-manifest.json. Game ZIPs: alihammoud21/playgta5, full-game-v1 on GitHub. The manifest lists 14 ZIPs, about 21.2 GB of downloads. Use Wi-Fi and allow roughly 25 GB free internal storage (plus an existing installation if replacing it). Files are checked and installed one part at a time. Downloads continue with a notification; Pause / Resume keeps progress.\n\nPowered by Muguet. This is an unofficial third-party mirror, not endorsed by Rockstar. Download only if you are entitled to use these assets. Android gameplay is still unverified.")
            .setNegativeButton("Cancel", null).setPositiveButton("Download", (dialog, which) -> {
                try {
                    if (Build.VERSION.SDK_INT >= 33 && checkSelfPermission(android.Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
                        requestPermissions(new String[]{android.Manifest.permission.POST_NOTIFICATIONS}, 2);
                    }
                    startForegroundService(new Intent(this, GameDownloadService.class).setAction(GameDownloadService.START));
                    play.setEnabled(false); importButton.setEnabled(false); downloadButton.setEnabled(false); discardButton.setEnabled(false);
                } catch (Exception e) { error(e); }
            }).show();
    }
    private void discardDownload() {
        if (importing || !GameDownloadService.reserve()) return;
        importing = true;
        downloadState(GameDownloadService.current());
        status.setText("Discarding unfinished download…");
        worker.execute(() -> {
            Exception failure = null;
            try { GameInstaller.discardPartial(getFilesDir()); } catch (Exception e) { failure = e; }
            Exception result = failure;
            runOnUiThread(() -> { importing = false; GameDownloadService.localFinished(); ready(); downloadState(GameDownloadService.current()); if (result != null) error(result); });
        });
    }
    private void downloadState(GameDownloadService.State state) {
        if (isDestroyed()) return;
        boolean busy = importing || state.running || GameDownloadService.busy();
        boolean partial = GameInstaller.hasPartial(getFilesDir());
        downloadButton.setEnabled(!busy);
        pauseButton.setVisibility(state.running ? View.VISIBLE : View.GONE);
        pauseButton.setEnabled(state.running);
        discardButton.setVisibility(partial && !state.running ? View.VISIBLE : View.GONE);
        discardButton.setEnabled(!busy);
        importButton.setEnabled(!busy && !partial);
        if (state.running) {
            play.setEnabled(false);
            progress.setIndeterminate(state.total <= 0);
            progress.setProgress(state.total <= 0 ? 0 : (int) Math.min(1000, 1000.0 * state.done / state.total));
            status.setText(state.text + (state.total > 0 ? String.format(java.util.Locale.ROOT, "\n%.2f / %.2f GB", state.done / 1e9, state.total / 1e9) : ""));
        } else if (!importing) {
            progress.setIndeterminate(false);
            ready();
            if (!state.text.isEmpty()) status.setText(state.text);
            else if (partial) status.setText("Partial game download found. Tap Download / Resume to continue, or discard it to import a ZIP.");
        }
    }
    @Override protected void onStart() { super.onStart(); GameDownloadService.observe(downloadObserver); }
    @Override protected void onStop() { GameDownloadService.unobserve(downloadObserver); super.onStop(); }

    private File config() { return new File(getFilesDir(), "config.json"); }
    private Spinner choice(LinearLayout column, String label, String[] options, int selected) {
        TextView text = new TextView(this); text.setText(label); column.addView(text);
        Spinner spinner = new Spinner(this);
        ArrayAdapter<String> adapter = new ArrayAdapter<>(this, android.R.layout.simple_spinner_item, options);
        adapter.setDropDownViewResource(android.R.layout.simple_spinner_dropdown_item);
        spinner.setAdapter(adapter); spinner.setSelection(selected); column.addView(spinner);
        return spinner;
    }
    private EditText field(LinearLayout column, String label, String value) {
        TextView text = new TextView(this); text.setText(label); column.addView(text);
        EditText edit = new EditText(this); edit.setSingleLine(true); edit.setText(value); column.addView(edit);
        return edit;
    }
    private CheckBox check(LinearLayout column, String label, boolean value) {
        CheckBox box = new CheckBox(this); box.setText(label); box.setChecked(value); column.addView(box); return box;
    }
    private void ready() {
        try {
            boolean ready = nativePrepare(getFilesDir().getAbsolutePath());
            play.setEnabled(ready && !importing && !GameDownloadService.busy() && !GameInstaller.needsRecovery(getFilesDir()));
            status.setText(ready ? "Game data ready." : "Download or import game data to begin.");
        } catch (Exception e) { play.setEnabled(false); error(e); }
    }
    private void error(Exception e) {
        if (!isFinishing() && !isDestroyed()) new AlertDialog.Builder(this).setTitle("GTA 5").setMessage(e.getMessage()).setPositiveButton("OK", null).show();
    }
    // Called by Rust on the import worker, never directly updates UI there.
    public void importProgress(long done, long total) {
        runOnUiThread(() -> {
            progress.setIndeterminate(false);
            progress.setProgress(total == 0 ? 0 : (int) (1000.0 * done / total));
            status.setText("Extracting: " + (done >> 20) + " / " + (total >> 20) + " MB");
        });
    }
    @Override protected void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        if (request != PICK_ZIP || result != RESULT_OK || data == null || data.getData() == null || importing || GameInstaller.hasPartial(getFilesDir()) || !GameDownloadService.reserve()) return;
        Uri uri = data.getData();
        importing = true;
        play.setEnabled(false); importButton.setEnabled(false); downloadButton.setEnabled(false); discardButton.setEnabled(false); progress.setIndeterminate(true);
        status.setText("Copying ZIP to private cache. Keep the app open; allow space for the ZIP and extracted data.");
        worker.execute(() -> {
            File archive = new File(getCacheDir(), "game-import.zip");
            Exception failure = null;
            try {
                try (InputStream input = getContentResolver().openInputStream(uri); OutputStream output = new FileOutputStream(archive)) {
                    if (input == null) throw new IOException("Cannot open the selected ZIP");
                    byte[] buffer = new byte[1 << 20];
                    int n;
                    while ((n = input.read(buffer)) != -1) output.write(buffer, 0, n);
                }
                nativeImport(archive.getAbsolutePath());
            } catch (Exception e) { failure = e; }
            finally { archive.delete(); }
            Exception error = failure;
            runOnUiThread(() -> {
                importing = false; GameDownloadService.localFinished(); importButton.setEnabled(true); progress.setIndeterminate(false);
                ready();
                downloadState(GameDownloadService.current());
                if (error != null) error(error);
            });
        });
    }
    @Override protected void onDestroy() { worker.shutdown(); super.onDestroy(); }
}
