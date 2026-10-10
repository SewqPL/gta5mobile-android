package dev.muguet;

import android.app.Activity;
import android.app.AlertDialog;
import android.content.Intent;
import android.net.Uri;
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
    private Button play, importButton;
    private ProgressBar progress;
    private Spinner mode, fps, heap;
    private EditText scale, server, name;
    private CheckBox mute, multiplayer, host;
    private JSONObject settings;
    private boolean importing;

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
        title.setText("Muguet — Android (experimental)");
        title.setTextSize(24);
        column.addView(title);
        TextView warning = new TextView(this);
        warning.setText("ARM64 / Android 10+ / Vulkan. A high-end phone with 8–12 GB RAM is recommended, not a guarantee of compatibility. Import your own web-port ZIP (data/ and b/<build>/game.wasm + shaders/); a normal PC installation is not enough. First launch compiles the game and may take several minutes.");
        column.addView(warning);
        status = new TextView(this);
        column.addView(status);
        progress = new ProgressBar(this, null, android.R.attr.progressBarStyleHorizontal);
        progress.setMax(1000);
        column.addView(progress);
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
    }

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
            play.setEnabled(ready && !importing);
            status.setText(ready ? "Game data ready." : "Import game data to begin.");
        } catch (Exception e) { play.setEnabled(false); error(e); }
    }
    private void error(Exception e) {
        if (!isFinishing() && !isDestroyed()) new AlertDialog.Builder(this).setTitle("Muguet").setMessage(e.getMessage()).setPositiveButton("OK", null).show();
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
        if (request != PICK_ZIP || result != RESULT_OK || data == null || data.getData() == null || importing) return;
        Uri uri = data.getData();
        importing = true;
        play.setEnabled(false); importButton.setEnabled(false); progress.setIndeterminate(true);
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
                importing = false; importButton.setEnabled(true); progress.setIndeterminate(false);
                ready();
                if (error != null) error(error);
            });
        });
    }
    @Override protected void onDestroy() { worker.shutdown(); super.onDestroy(); }
}
