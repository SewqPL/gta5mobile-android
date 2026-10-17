package dev.muguet;

import android.app.Activity;
import android.app.AlertDialog;
import android.content.Intent;
import android.net.Uri;
import android.os.Bundle;
import android.widget.*;
import org.json.JSONArray;
import org.json.JSONObject;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

public final class LauncherActivity extends Activity {
    static { System.loadLibrary("muguet"); }
    private static final int PICK_ZIP = 1;
    private static final int PICK_MAGNET = 2;
    private static final int PICK_FOLDER = 3;
    private static final String DEFAULT_MAGNET =
        "magnet:?xt=urn:btih:60a2d95f23154e4f7c75a965307133c24d9c7178&dn=GrandV-assets"
        + "&tr=udp%3A%2F%2Ftracker.opentrackr.org%3A1337%2Fannounce"
        + "&tr=udp%3A%2F%2Ftracker.torrent.eu.org%3A451"
        + "&tr=http%3A%2F%2Ftracker.opentrackr.org%3A1337%2Fannounce"
        + "&tr=udp%3A%2F%2Fopen.stealth.si%3A80%2Fannounce"
        + "&tr=http%3A%2F%2Ftracker.qu.ax%3A6969%2Fannounce"
        + "&tr=udp%3A%2F%2Ftracker.skynetcloud.site%3A6969%2Fannounce"
        + "&tr=udp%3A%2F%2Ftracker.qu.ax%3A6969%2Fannounce"
        + "&tr=udp%3A%2F%2Ftracker.ducks.party%3A1984%2Fannounce"
        + "&tr=udp%3A%2F%2Ftracker.tryhackx.org%3A6969%2Fannounce"
        + "&tr=udp%3A%2F%2Ftracker.corpscorp.online%3A80%2Fannounce"
        + "&tr=udp%3A%2F%2Ftracker.bittor.pw%3A1337%2Fannounce"
        + "&tr=udp%3A%2F%2Ftracker-udp.gbitt.info%3A80%2Fannounce"
        + "&tr=udp%3A%2F%2Ftracker.nyaa.vc%3A6969%2Fannounce"
        + "&tr=udp%3A%2F%2Ftracker.gmi.gd%3A6969%2Fannounce"
        + "&tr=udp%3A%2F%2Ftracker2.dler.org%3A80%2Fannounce"
        + "&tr=udp%3A%2F%2Ftracker.dler.org%3A6969%2Fannounce"
        + "&tr=http%3A%2F%2Ftracker2.dler.org%3A80%2Fannounce"
        + "&tr=http%3A%2F%2Ftracker.dler.org%3A80%2Fannounce"
        + "&tr=http%3A%2F%2Ftracker.dler.com%3A6969%2Fannounce"
        + "&tr=http%3A%2F%2Ftracker.renfei.net%3A8080%2Fannounce"
        + "&tr=udp%3A%2F%2Ftracker.farted.net%3A6969%2Fannounce"
        + "&tr=udp%3A%2F%2Ftracker.peerfect.org%3A6969%2Fannounce"
        + "&tr=udp%3A%2F%2Ftracker.opentrackr.com%3A6969%2Fannounce"
        + "&tr=udp%3A%2F%2Ftracker.ilibr.org%3A6969%2Fannounce"
        + "&tr=udp%3A%2F%2Fmail.segso.net%3A6969%2Fannounce"
        + "&tr=https%3A%2F%2Ftracker.pmman.tech%3A443%2Fannounce"
        + "&tr=https%3A%2F%2Fht.therarbg.to%3A443%2Fannounce"
        + "&tr=http%3A%2F%2Fbittorrent-tracker.e-n-c-r-y-p-t.net%3A1337%2Fannounce"
        + "&tr=udp%3A%2F%2Fretracker01-msk-virt.corbina.net%3A80%2Fannounce"
        + "&tr=udp%3A%2F%2Ft.overflow.biz%3A6969%2Fannounce"
        + "&tr=http%3A%2F%2Ft.overflow.biz%3A6969%2Fannounce"
        + "&tr=udp%3A%2F%2Ftracker.004430.xyz%3A1337%2Fannounce"
        + "&tr=https%3A%2F%2Ftracker.nekomi.cn%3A443%2Fannounce"
        + "&tr=https%3A%2F%2F004430.xyz%3A443%2Fannounce"
        + "&tr=http%3A%2F%2F004430.xyz%3A80%2Fannounce"
        + "&tr=udp%3A%2F%2Fipv4announce.sktorrent.eu%3A6969%2Fannounce"
        + "&tr=http%3A%2F%2Fipv4announce.sktorrent.eu%3A6969%2Fannounce"
        + "&tr=udp%3A%2F%2Fevan.im%3A6969%2Fannounce"
        + "&tr=udp%3A%2F%2Ftr4ck3r.duckdns.org%3A6969%2Fannounce"
        + "&tr=udp%3A%2F%2Fns575949.ip-51-222-82.net%3A6969%2Fannounce"
        + "&tr=https%3A%2F%2Ftr.nyacat.pw%3A443%2Fannounce"
        + "&tr=http%3A%2F%2Ftracker.mywaifu.best%3A6969%2Fannounce"
        + "&tr=http%3A%2F%2Ftr.nyacat.pw%3A80%2Fannounce"
        + "&tr=udp%3A%2F%2Ftracker.wildkat.net%3A6969%2Fannounce"
        + "&tr=udp%3A%2F%2Fopen.ftorrent.com%3A443%2Fannounce"
        + "&tr=udp%3A%2F%2Fmartin-gebhardt.eu%3A25%2Fannounce"
        + "&tr=https%3A%2F%2Ftracker.7471.top%3A443%2Fannounce"
        + "&tr=http%3A%2F%2Ftracker.waaa.moe%3A6969%2Fannounce"
        + "&tr=http%3A%2F%2Ftracker.dhitechnical.com%3A6969%2Fannounce"
        + "&tr=http%3A%2F%2Fopen.tracker.cl%3A1337%2Fannounce"
        + "&tr=udp%3A%2F%2Fadmin.52ywp.com%3A6969%2Fannounce"
        + "&tr=https%3A%2F%2Ftracker.zhuqiy.com%3A443%2Fannounce"
        + "&tr=udp%3A%2F%2Ftracker.aruku.ovh%3A8081%2Fannounce"
        + "&tr=https%3A%2F%2Ftracker.foreverpirates.co%3A443%2Fannounce"
        + "&tr=https%3A%2F%2Fopen.ftorrent.com%3A443%2Fannounce"
        + "&tr=udp%3A%2F%2Ftracker.torrents.observer%3A80%2Fannounce"
        + "&tr=udp%3A%2F%2Ftracker.nyaa.net%3A6969%2Fannounce"
        + "&tr=udp%3A%2F%2Ftracker.k.vu%3A6969%2Fannounce"
        + "&tr=udp%3A%2F%2Ftracker.cn.nyaa.net%3A6969%2Fannounce"
        + "&tr=udp%3A%2F%2Fkolankoalastree.newtrackon.co.nz%3A1337%2Fannounce"
        + "&tr=http%3A%2F%2Ftracker.zhuqiy.dgj055.icu%3A80%2Fannounce"
        + "&tr=https%3A%2F%2Ft.213891.xyz%3A443%2Fannounce"
        + "&tr=http%3A%2F%2F1337.abcvg.info%3A80%2Fannounce";
    // Every graphics knob the web port exposes, in the same order, with the same
    // choices, taken from graphics_settings.js on the game data. Columns are
    // label, engine flag, then the engine's own values. DISPLAY below renames
    // those values for people who do not know what a LOD bias is; index N of a
    // DISPLAY row is the same knob as GRAPHICS[i][2 + N].
    private static final String[][] GRAPHICS = {
        {"Texture quality", "-textureQuality", "auto", "0", "1", "2"},
        {"Reflection quality (mirrors, glass)", "-reflectionQuality", "auto", "0", "1", "2", "3"},
        {"Water quality", "-waterQuality", "auto", "0", "1"},
        {"Particle quality (smoke, sparks)", "-particleQuality", "auto", "0", "1", "2"},
        {"Grass quality", "-grassQuality", "auto", "0", "1", "2", "3"},
        {"Texture sharpness at angles", "-anisotropicQualityLevel", "auto", "0", "2", "4", "8", "16"},
        {"Scenery detail at a distance", "-lodScale", "auto", "0", "0.25", "0.5", "0.75", "1"},
        {"Pedestrian detail at a distance", "-pedLodBias", "auto", "0", "0.25", "0.5", "0.75", "1"},
        {"Vehicle detail at a distance", "-vehicleLodBias", "auto", "0", "0.25", "0.5", "0.75", "1"},
        {"City density (crowds, traffic)", "-cityDensity", "auto", "0", "0.25", "0.5", "0.75", "1"},
        {"Pedestrian variety", "-pedVariety", "auto", "0", "0.25", "0.5", "0.75", "1"},
        {"Vehicle variety", "-vehicleVariety", "auto", "0", "0.25", "0.5", "0.75", "1"},
        {"Fullscreen effects (bloom, grade)", "-postFX", "auto", "0", "1", "2", "3"},
        {"Motion blur", "-MotionBlurStrength", "auto", "0", "0.25", "0.5", "1"},
        {"Depth of field (blurred background)", "-noInGameDOF", "auto", "1", "0"},
        {"Shadow quality", "-shadowQuality", "auto", "-1", "0", "1", "2"},
        {"Shadow edges", "-shadowSoftness", "auto", "0", "1", "2", "3", "4", "5"},
        {"Ambient occlusion (corners and creases)", "-SSAO", "auto", "0", "1", "2"},
        {"Render distance (how far you can see)", "-farclip", "auto", "10", "25", "50", "100", "200", "500", "1000", "2000", "5000"},
        {"Fog you can walk through", "-fogVolumes", "auto", "0", "1"},
        {"Shadows from smoke and fire", "-particleShadows", "auto", "0", "1"},
        {"Long shadows from the sun", "-shadowLongShadows", "auto", "0", "1"},
        {"Loading assets ahead of you", "-HDStreamingInFlight", "auto", "0", "1"},
        {"Lights on far-away buildings", "-nolodlights", "auto", "on", "off"},
    };
    private static final String[][] DISPLAY = {
        {"Automatic", "Very low", "Low", "High"},
        {"Automatic", "Off", "Low", "Medium", "High"},
        {"Automatic", "Off", "High"},
        {"Automatic", "Off", "Low", "Medium", "High"},
        {"Automatic", "Off", "Very low", "Low", "High"},
        {"Automatic", "Off", "2x", "4x", "8x", "16x"},
        {"Automatic", "Lowest (fastest)", "Low", "Medium", "High", "Full"},
        {"Automatic", "Lowest (fastest)", "Low", "Medium", "High", "Full"},
        {"Automatic", "Lowest (fastest)", "Low", "Medium", "High", "Full"},
        {"Automatic", "Empty", "Quiet", "Medium", "Busy", "Full"},
        {"Automatic", "None", "Low", "Medium", "High", "Full"},
        {"Automatic", "None", "Low", "Medium", "High", "Full"},
        {"Automatic", "Off", "Low", "Medium", "High"},
        {"Automatic", "Off", "Light", "Medium", "Full"},
        {"Automatic", "Off", "On"},
        {"Automatic", "None at all", "Off (fewer)", "Low", "High"},
        {"Automatic", "Hard edges", "Hard", "Normal", "Soft", "Softer", "Softest"},
        {"Automatic", "Off", "Low", "High"},
        {"Automatic", "Extreme close", "Very close", "Close", "Near", "Short", "Medium", "Long", "Very long", "Full"},
        {"Automatic", "Off", "On"},
        {"Automatic", "Off", "On"},
        {"Automatic", "Off", "On"},
        {"Automatic", "Off", "On"},
        {"Automatic", "On", "Off"},
    };
    // Same three presets as the web launcher. "Low" is the phone default; farclip
    // is 500 rather than the web preset's 200 because 200 pops badly at street
    // level, depth of field is inverted to match the engine's own reading, and
    // shadow quality uses -1, the value graphics_settings.js passes when the
    // web launcher turns shadows off entirely.
    private static final String[][] PRESETS = {
        {"0", "0", "0", "0", "0", "0", "0", "0", "0", "0.25", "0", "0", "0", "0", "1", "-1", "0", "0", "500", "0", "0", "0", "0", "off"},
        {"auto", "auto", "auto", "auto", "auto", "auto", "auto", "auto", "auto", "auto", "auto", "auto", "auto", "auto", "auto", "auto", "auto", "auto", "auto", "auto", "auto", "auto", "auto", "auto"},
        {"2", "3", "1", "2", "3", "16", "1", "1", "1", "1", "1", "1", "3", "1", "0", "2", "3", "2", "2000", "1", "1", "1", "1", "on"},
    };
    private Spinner[] graphicsSpinners;
    private Spinner graphicsPreset;
    private boolean loadingGraphics;

    private static native boolean nativePrepare(String directory);
    private native void nativeImport(String archive);
    private final ExecutorService worker = Executors.newSingleThreadExecutor();
    private TextView status;
    private Button play, playEngine, importButton, clearFolderButton, grantAccessButton;
    private TextView folderStatus;
    private ProgressBar progress;
    private Spinner mode, fps, heap;
    private EditText scale, server, name, magnetField;
    private CheckBox mute, multiplayer, host;
    private SeekBar sensitivity;
    private TextView sensitivityLabel;
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
        title.setText("GTA 5 — Android (experimental)");
        title.setTextSize(24);
        column.addView(title);
        TextView warning = new TextView(this);
        warning.setText("ARM64 / Android 10+ / Vulkan. A high-end phone with 8–12 GB RAM is recommended, not a guarantee of compatibility. Supply your own web-port data: import a ZIP, or point the app at a folder that already holds data/ and b/<build>/ with game.wasm and shaders inside. A normal PC installation is not enough. First launch compiles the game and may take several minutes.");
        column.addView(warning);
        status = new TextView(this);
        column.addView(status);
        progress = new ProgressBar(this, null, android.R.attr.progressBarStyleHorizontal);
        progress.setMax(1000);
        column.addView(progress);
        // Game data comes from the user's own files: either a ZIP to extract or a folder
        // to use where it lies. The app never fetches game data from any server,
        // which keeps this project clear of mirroring Rockstar's content.
        importButton = new Button(this);
        importButton.setText("Import game ZIP");
        importButton.setOnClickListener(v -> {
            Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
            intent.addCategory(Intent.CATEGORY_OPENABLE);
            intent.setType("*/*");
            startActivityForResult(intent, PICK_ZIP);
        });
        column.addView(importButton);
        // Not everyone has the game data as a ZIP. A folder that already holds
        // data/ and b/<build>/ works just as well, and is used where it lies, so
        // nothing is copied and the phone needs no free space for it.
        Button folderButton = new Button(this);
        folderButton.setText("Use a game files folder");
        folderButton.setOnClickListener(v -> {
            Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE);
            intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION
                | Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION);
            startActivityForResult(intent, PICK_FOLDER);
        });
        column.addView(folderButton);
        folderStatus = new TextView(this);
        column.addView(folderStatus);
        clearFolderButton = new Button(this);
        clearFolderButton.setText("Go back to the imported copy");
        clearFolderButton.setOnClickListener(v -> { settings.remove("game_folder_uri"); settings.remove("game_folder_path"); saveSettings(); ready(); });
        column.addView(clearFolderButton);
        // A picked /sdcard folder can be stat-ed but not opened while another app
        // owns the files, and a SAF grant does not change that. All-files access
        // does, and it is the user's decision, not ours.
        grantAccessButton = new Button(this);
        grantAccessButton.setText("Grant file access");
        grantAccessButton.setVisibility(android.view.View.GONE);
        grantAccessButton.setOnClickListener(v -> {
            try {
                startActivity(new Intent(android.provider.Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION,
                    Uri.parse("package:" + getPackageName())));
            } catch (Exception e) { status.setText("Grant 'All files access' to GTA 5 in Settings > Apps > Special access."); }
        });
        column.addView(grantAccessButton);
        try { settings = new JSONObject(new String(Files.readAllBytes(config().toPath()), StandardCharsets.UTF_8)); }
        catch (Exception ignored) { settings = new JSONObject(); }
        // The user pastes their own magnet link; we only hand it to whatever
        // torrent app they already have. Nothing is fetched or hosted in-app.
        magnetField = field(column, "Game torrent (paste your own magnet link)", settings.optString("last_magnet", DEFAULT_MAGNET));
        Button torrentButton = new Button(this);
        torrentButton.setText("Open torrent client");
        torrentButton.setOnClickListener(v -> {
            String link = magnetField.getText().toString().trim();
            if (!link.startsWith("magnet:?")) {
                status.setText("Paste a magnet:? link first.");
                return;
            }
            try {
                settings.put("last_magnet", link);
                Intent torrent = new Intent(Intent.ACTION_VIEW, Uri.parse(link));
                torrent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
                startActivity(torrent);
            } catch (Exception e) {
                status.setText("No torrent app found. Install qBittorrent or Flud first.");
            }
        });
        column.addView(torrentButton);
        // Long magnets are painful to type on a phone. Pick a text file from
        // anywhere (Downloads, Drive) instead; the link itself never ships here.
        Button loadMagnetButton = new Button(this);
        loadMagnetButton.setText("Load magnet from file");
        loadMagnetButton.setOnClickListener(v -> {
            Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
            intent.addCategory(Intent.CATEGORY_OPENABLE);
            intent.setType("*/*");
            startActivityForResult(intent, PICK_MAGNET);
        });
        column.addView(loadMagnetButton);
        mode = choice(column, "Start mode", new String[]{"Story", "Sandbox"}, settings.optInt("start_mode", 2) == 1 ? 0 : 1);
        fps = choice(column, "Frame limit", new String[]{"30 FPS", "60 FPS"}, settings.optInt("fps_limit", 30) <= 30 ? 0 : 1);
        heap = choice(column, "Maximum game heap (not total app memory)", new String[]{"3 GB", "4 GB", "5 GB", "6 GB", "7 GB", "8 GB"}, Math.max(0, Math.min(5, settings.optInt("android_heap_gb", 5) - 3)));
        // Graphics live here, not in a save file: the web port's graphics_settings.js
        // says "apply after custom args; never edit SGTA saves", and the engine rewrites
        // its own settings.xml on every save.
        TextView graphicsHeader = new TextView(this);
        graphicsHeader.setText("Graphics — passed to the engine as arguments, the same knobs the web launcher exposes");
        column.addView(graphicsHeader);
        graphicsPreset = choice(column, "Graphics preset", new String[]{"Low (phone default)", "Engine default", "Max"}, 0);
        graphicsPreset.setOnItemSelectedListener(new AdapterView.OnItemSelectedListener() {
            @Override public void onItemSelected(AdapterView<?> parent, android.view.View view, int position, long id) {
                if (!loadingGraphics) applyPreset(position);
            }
            @Override public void onNothingSelected(AdapterView<?> parent) {}
        });
        graphicsSpinners = new Spinner[GRAPHICS.length];
        loadingGraphics = true;
        for (int i = 0; i < GRAPHICS.length; i++) {
            String[] row = GRAPHICS[i];
            graphicsSpinners[i] = choice(column, row[0], DISPLAY[i], 0);
        }
        restoreGraphics();
        loadingGraphics = false;
        scale = field(column, "Render scale (0.25–1.0)", Double.toString(settings.optDouble("scale", 0.5)));
        scale.setInputType(android.text.InputType.TYPE_CLASS_NUMBER | android.text.InputType.TYPE_NUMBER_FLAG_DECIMAL);
        mute = check(column, "Mute audio", settings.optBoolean("mute", false));
        multiplayer = check(column, "Multiplayer (Muguet UDP, not GTA Online)", settings.optBoolean("multiplayer", false));
        host = check(column, "Host a session", settings.optBoolean("host", false));
        server = field(column, "Server address (when joining)", settings.optString("server", ""));
        name = field(column, "Player name", settings.optString("player_name", ""));
        sensitivityLabel = new TextView(this);
        sensitivityLabel.setText("Look sensitivity (touch drag)");
        column.addView(sensitivityLabel);
        sensitivity = new SeekBar(this);
        sensitivity.setMax(15);
        int savedLook = LookSensitivity.toSlider((float) settings.optDouble("touch_sensitivity", 1.0));
        sensitivity.setProgress(savedLook);
        sensitivity.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener() {
            @Override public void onProgressChanged(SeekBar bar, int value, boolean fromUser) {
                int level = Math.max(1, value);
                if (level != value) bar.setProgress(level); // 0 leaves touch look unusable.
                sensitivityLabel.setText("Look sensitivity (touch drag): " + String.format(java.util.Locale.ROOT, "%.2fx", LookSensitivity.fromSlider(level)));
            }
            @Override public void onStartTrackingTouch(SeekBar bar) {}
            @Override public void onStopTrackingTouch(SeekBar bar) {}
        });
        sensitivityLabel.setText("Look sensitivity (touch drag): " + String.format(java.util.Locale.ROOT, "%.2fx", LookSensitivity.fromSlider(sensitivity.getProgress())));
        column.addView(sensitivity);
        play = new Button(this);
        play.setText("Play (wasmtime engine)");
        play.setOnClickListener(v -> launch(GameActivity.class));
        column.addView(play);
        playEngine = new Button(this);
        playEngine.setText("Play (native wasm2c engine)");
        playEngine.setOnClickListener(v -> launch(EngineActivity.class));
        column.addView(playEngine);
        ready();
    }

    // Persist the chosen settings, then start the requested game activity.
    private void launch(Class<?> activity) {
        try {
            double renderScale = Double.parseDouble(scale.getText().toString());
            if (!Double.isFinite(renderScale) || renderScale < 0.25 || renderScale > 1) throw new IllegalArgumentException("Render scale must be between 0.25 and 1.0");
            settings.put("scale", renderScale);
            settings.put("start_mode", mode.getSelectedItemPosition() + 1);
            settings.put("fps_limit", fps.getSelectedItemPosition() == 0 ? 30 : 60);
            settings.put("android_heap_gb", heap.getSelectedItemPosition() + 3);
            settings.put("texture_quality", settings.optInt("texture_quality", 0));
            settings.put("graphics", graphicsChoices());
            settings.put("extra_args", graphicsArgs());
            settings.put("fullscreen", true);
            settings.put("mute", mute.isChecked());
            settings.put("multiplayer", multiplayer.isChecked());
            settings.put("host", host.isChecked());
            settings.put("server", server.getText().toString().trim());
            settings.put("player_name", name.getText().toString().trim());
            settings.put("last_magnet", magnetField.getText().toString().trim());
            settings.put("touch_sensitivity", LookSensitivity.fromSlider(sensitivity.getProgress()));
            saveSettings();
            startActivity(new Intent(this, activity));
        } catch (Exception e) { error(e); }
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
            String folder = folderPath();
            boolean pickedSomething = !settings.optString("game_folder_uri", "").isEmpty();
            if (folder == null) {
                folderStatus.setText(pickedSomething
                    ? "That folder cannot be read. Grant file access below, or pick another folder."
                    : "No game data yet. Import a ZIP, or pick a folder that holds data/ and b/.");
                clearFolderButton.setVisibility(pickedSomething ? android.view.View.VISIBLE : android.view.View.GONE);
                grantAccessButton.setVisibility(pickedSomething && !hasAllFilesAccess() ? android.view.View.VISIBLE : android.view.View.GONE);
                play.setEnabled(false); playEngine.setEnabled(false);
                status.setText(pickedSomething ? "Game folder not readable." : "Import game data to begin.");
                return;
            }
            grantAccessButton.setVisibility(android.view.View.GONE);
            if (folder.isEmpty()) {
                folderStatus.setText("Using the copy imported from a ZIP.");
                clearFolderButton.setVisibility(android.view.View.GONE);
            } else {
                folderStatus.setText("Game folder: " + folder);
                clearFolderButton.setVisibility(android.view.View.VISIBLE);
                if (!folder.equals(settings.optString("game_folder_path", ""))) {
                    settings.put("game_folder_path", folder);
                    saveSettings();
                }
            }
            boolean ready = nativePrepare(getFilesDir().getAbsolutePath());
            boolean canPlay = ready && !importing;
            play.setEnabled(canPlay);
            playEngine.setEnabled(canPlay);
            status.setText(ready ? "Game data ready." : "Import game data to begin.");
        } catch (Exception e) { play.setEnabled(false); playEngine.setEnabled(false); error(e); }
    }
    // The picker hands back a tree Uri but the engine reads plain paths, so the
    // tree document id is mapped back to /storage/emulated/0/... . Shared storage
    // keeps a real path an app can read; an SD card or USB volume has none, so it
    // is reported rather than silently loading nothing.
    // Returns "" when no folder was ever picked, and null when the picked one is unusable.
    private String folderPath() {
        String stored = settings.optString("game_folder_uri", "");
        if (stored.isEmpty()) return "";
        Uri tree = Uri.parse(stored);
        try {
            getContentResolver().takePersistableUriPermission(tree,
                Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION);
        } catch (SecurityException ignored) {
            // The grant was revoked or the pick was never persistable; the check below decides.
        }
        try {
            String id = android.provider.DocumentsContract.getTreeDocumentId(tree);
            int colon = id.indexOf(':');
            if (colon < 0 || !"primary".equals(id.substring(0, colon))) return null;
            File base = new File("/storage/emulated/0/" + id.substring(colon + 1));
            for (File candidate : new File[]{base, new File(base, "game")}) {
                if (readable(candidate)) return candidate.getAbsolutePath();
            }
            return null;
        } catch (Exception e) { return null; }
    }
    // Opening the manifest is the test, not listing it: on /sdcard an app can stat
    // a file it is not allowed to read, and that false "yes" is what used to look
    // like a broken install instead of a missing permission.
    private static boolean readable(File dir) {
        try (InputStream in = new FileInputStream(new File(dir, "data/manifest.json"))) { return in.read() >= 0; }
        catch (Exception e) { return false; }
    }
    private static boolean hasAllFilesAccess() {
        return android.os.Build.VERSION.SDK_INT < 30 || new android.os.Environment().isExternalStorageManager();
    }
    private void saveSettings() {
        try {
            File temp = new File(getFilesDir(), "config.json.new");
            Files.write(temp.toPath(), settings.toString(2).getBytes(StandardCharsets.UTF_8));
            Files.move(temp.toPath(), config().toPath(), java.nio.file.StandardCopyOption.REPLACE_EXISTING);
        } catch (Exception e) { error(e); }
    }
    // Each spinner holds one knob's value; the engine only ever sees the flags
    // assembled here, so nothing depends on the game persisting its own settings.
    private void applyPreset(int preset) {
        for (int i = 0; i < GRAPHICS.length; i++) select(graphicsSpinners[i], GRAPHICS[i], PRESETS[preset][i]);
    }
    private void restoreGraphics() {
        JSONObject saved = settings.optJSONObject("graphics");
        for (int i = 0; i < GRAPHICS.length; i++) {
            String flag = GRAPHICS[i][1];
            select(graphicsSpinners[i], GRAPHICS[i], saved == null ? PRESETS[0][i] : saved.optString(flag, PRESETS[0][i]));
        }
    }
    private static void select(Spinner spinner, String[] row, String value) {
        for (int i = 2; i < row.length; i++) {
            if (row[i].equals(value)) { spinner.setSelection(i - 2); return; }
        }
        spinner.setSelection(0);
    }
    // The spinners show friendly names, so the engine's value is read back by
    // position: DISPLAY index N is GRAPHICS[i][2 + N].
    private String graphicsValue(int row) {
        return GRAPHICS[row][2 + graphicsSpinners[row].getSelectedItemPosition()];
    }
    private JSONObject graphicsChoices() {
        JSONObject out = new JSONObject();
        try {
            for (int i = 0; i < GRAPHICS.length; i++) {
                String value = graphicsValue(i);
                if (!"auto".equals(value)) out.put(GRAPHICS[i][1], value);
            }
        } catch (Exception e) { error(e); }
        return out;
    }
    private JSONArray graphicsArgs() {
        JSONArray out = new JSONArray();
        try {
            for (int i = 0; i < GRAPHICS.length; i++) {
                String value = graphicsValue(i);
                if ("auto".equals(value)) continue;
                out.put("off".equals(value) ? GRAPHICS[i][1] : GRAPHICS[i][1] + "=" + value);
            }
        } catch (Exception e) { error(e); }
        return out;
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
        if (request == PICK_MAGNET) {
            if (result != RESULT_OK || data == null || data.getData() == null) return;
            try (InputStream in = getContentResolver().openInputStream(data.getData())) {
                if (in == null) return;
                // Pull the magnet out of whatever the file holds (plain text, a
                // .torrent with a magnet comment, a wrapper page) and keep only it.
                String text = new String(in.readAllBytes(), StandardCharsets.UTF_8);
                int at = text.indexOf("magnet:?");
                if (at < 0) { status.setText("No magnet link found in that file."); return; }
                int end = at;
                while (end < text.length() && !Character.isWhitespace(text.charAt(end))) end++;
                magnetField.setText(text.substring(at, end));
                status.setText("Magnet loaded. Tap Open torrent client.");
            } catch (Exception e) { status.setText("Could not read that file."); }
            return;
        }
        if (request == PICK_FOLDER) {
            if (result != RESULT_OK || data == null || data.getData() == null) {
                if (result != RESULT_CANCELED) status.setText("Folder pick cancelled.");
                return;
            }
            Uri tree = data.getData();
            int keep = data.getFlags() & (Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION);
            try {
                // Without a persisted grant the folder is readable for this one
                // launch only, so the picker would have to be used every time.
                getContentResolver().takePersistableUriPermission(tree, keep);
                String path = folderPath();
                if (path == null) {
                    status.setText("That folder has no data/manifest.json. Pick the folder holding data/ and b/.");
                    return;
                }
                settings.put("game_folder_uri", tree.toString());
                // The game runs in its own process, which cannot see anything the
                // launcher passes it, so the resolved path is what it reads.
                settings.put("game_folder_path", path);
                saveSettings();
                ready();
            } catch (Exception e) { status.setText("Android would not grant lasting access to that folder."); }
            return;
        }
        if (request != PICK_ZIP) return;
        if (result != RESULT_OK || data == null || data.getData() == null) {
            // Cancelled or nothing usable picked. Say so instead of returning silently.
            if (result != RESULT_CANCELED) status.setText("Import cancelled.");
            return;
        }
        // Every one of these used to return without a word, so tapping Import
        // appeared to do nothing at all. Tell the user which one blocked them.
        if (importing) { status.setText("An import is already running."); return; }
        Uri uri = data.getData();
        importing = true;
        play.setEnabled(false); playEngine.setEnabled(false); importButton.setEnabled(false); progress.setIndeterminate(true);
        status.setText("Copying ZIP to private cache. Keep the app open; allow space for the ZIP and extracted data.");
        worker.execute(() -> {
            File archive = new File(getCacheDir(), "game-import.zip");
            Exception failure = null;
            long lastReported = 0;
            try {
                // Copying a multi-GB ZIP out of the content provider takes minutes,
                // so report it. The provider often cannot give a size, in which case
                // the copy stays indeterminate rather than showing a bogus fraction.
                long total = 0;
                try (android.database.Cursor c = getContentResolver().query(uri, null, null, null, null)) {
                    if (c != null && c.moveToFirst()) {
                        int column = c.getColumnIndex(android.provider.OpenableColumns.SIZE);
                        if (column >= 0 && !c.isNull(column)) total = c.getLong(column);
                    }
                }
                final long known = total;
                runOnUiThread(() -> {
                    if (known > 0) progress.setIndeterminate(false);
                    status.setText(known > 0
                        ? "Copying ZIP: 0 / " + (known >> 20) + " MB"
                        : "Copying ZIP to private cache. Keep the app open; allow space for the ZIP and extracted data.");
                });
                long copied = 0;
                try (InputStream input = getContentResolver().openInputStream(uri); OutputStream output = new FileOutputStream(archive)) {
                    if (input == null) throw new IOException("Cannot open the selected ZIP");
                    byte[] buffer = new byte[1 << 20];
                    int n;
                    while ((n = input.read(buffer)) != -1) {
                        output.write(buffer, 0, n);
                        copied += n;
                        if (known > 0 && copied - lastReported >= 32L << 20) {
                            lastReported = copied;
                            final long at = copied;
                            runOnUiThread(() -> {
                                progress.setIndeterminate(false);
                                progress.setProgress((int) (1000.0 * at / known));
                                status.setText("Copying ZIP: " + (at >> 20) + " / " + (known >> 20) + " MB");
                            });
                        }
                    }
                }
                runOnUiThread(() -> {
                    status.setText("Extracting (this can take several minutes)…");
                    progress.setIndeterminate(true);
                });
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
    @Override protected void onResume() {
        super.onResume();
        // Returning from the All-files settings screen has to re-test the folder.
        if (settings != null) ready();
    }
    @Override protected void onDestroy() { worker.shutdown(); super.onDestroy(); }
}
