package dev.muguet;

import org.json.JSONArray;
import org.json.JSONObject;
import java.io.*;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.security.MessageDigest;
import java.util.*;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.zip.CRC32;
import java.util.zip.ZipEntry;
import java.util.zip.ZipFile;

// Platform-independent installer: never execute release helpers or import each part separately.
final class GameInstaller {
    static final String RELEASE = "https://github.com/alihammoud21/playgta5/releases/download/full-game-v1/";
    static final String MANIFEST_URL = "https://gcode.bytarch.com/download-manifest.json";
    private static final String PREFIX = "mirror/playgta5.com/";
    private static final long GIB = 1L << 30, RESERVE = 2 * GIB, MAX_GAME_BYTES = 26 * GIB;
    private static final int BUFFER = 1 << 20, MAX_FILES = 30000;
    interface Listener { void update(String text, long done, long total); }
    interface Connections { HttpURLConnection open(URL url) throws IOException; }
    static final class Part {
        final String name, sha256, url;
        final long bytes;
        Part(String name, long bytes, String sha256, String url) {
            this.name = name; this.bytes = bytes; this.sha256 = sha256; this.url = url;
        }
    }
    static final class Manifest {
        final List<Part> parts;
        final long bytes, largest;
        final String fingerprint;
        Manifest(String text) throws Exception {
            JSONObject json = new JSONObject(text);
            if (json.getInt("version") != 1 || !"alihammoud21/playgta5".equals(json.getString("repository")) ||
                    !"full-game-v1".equals(json.getString("tag"))) throw new IOException("Unsupported game release manifest.");
            JSONArray array = json.getJSONArray("parts");
            if (array.length() < 1 || array.length() > 32) throw new IOException("Invalid number of release parts.");
            parts = new ArrayList<>();
            long sum = 0, maximum = 0;
            StringBuilder identity = new StringBuilder("1:alihammoud21/playgta5:full-game-v1\n");
            for (int i = 0; i < array.length(); i++) {
                JSONObject entry = array.getJSONObject(i);
                String name = entry.getString("name"), url = entry.getString("url");
                String expected = String.format(Locale.ROOT, "playgta5-part-%02d-of-%02d.zip", i + 1, array.length());
                String sha = entry.getString("sha256").toLowerCase(Locale.ROOT);
                long size = positiveLong(entry, "bytes");
                if (!expected.equals(name) || !(RELEASE + name).equals(url) || size > 2 * GIB ||
                        !sha.matches("[0-9a-f]{64}")) throw new IOException("Invalid release part: " + name);
                parts.add(new Part(name, size, sha, url));
                sum += size; maximum = Math.max(maximum, size);
                identity.append(name).append(':').append(size).append(':').append(sha).append('\n');
            }
            if (sum > MAX_GAME_BYTES) throw new IOException("Release exceeds the supported size limit.");
            bytes = sum; largest = maximum;
            fingerprint = hex(MessageDigest.getInstance("SHA-256").digest(identity.toString().getBytes(StandardCharsets.UTF_8)));
        }
    }
    private final File files, work, stage;
    private final Listener listener;
    private final Connections connections;
    private final byte[] buffer = new byte[BUFFER];
    private final AtomicBoolean cancelled = new AtomicBoolean();
    private volatile HttpURLConnection activeConnection;
    private long lastUpdate;

    GameInstaller(File files, Listener listener) { this(files, listener, url -> (HttpURLConnection) url.openConnection()); }
    GameInstaller(File files, Listener listener, Connections connections) {
        this.files = files; this.work = new File(files, "game-download"); this.stage = new File(work, "game");
        this.listener = listener; this.connections = connections;
    }
    void cancel() {
        cancelled.set(true);
        HttpURLConnection connection = activeConnection;
        if (connection != null) connection.disconnect();
    }
    private void checkCancelled() throws InterruptedIOException {
        if (cancelled.get() || Thread.currentThread().isInterrupted()) throw new InterruptedIOException("Download paused. Tap Download / Resume to continue.");
    }
    private void update(String text, long done, long total, boolean force) {
        long now = System.nanoTime();
        if (force || now - lastUpdate >= 500_000_000L) { lastUpdate = now; listener.update(text, done, total); }
    }
    static long positiveLong(JSONObject object, String key) throws Exception {
        Object value = object.get(key);
        if (!(value instanceof Number)) throw new IOException("Invalid numeric value: " + key);
        long number = ((Number) value).longValue();
        if (number <= 0 || ((Number) value).doubleValue() != (double) number) throw new IOException("Invalid size: " + key);
        return number;
    }
    private static String hex(byte[] bytes) {
        StringBuilder text = new StringBuilder(bytes.length * 2);
        for (byte b : bytes) text.append(String.format(Locale.ROOT, "%02x", b & 255));
        return text.toString();
    }
    private HttpURLConnection open(String address, long offset) throws Exception {
        URL url = new URL(address);
        for (int redirect = 0; redirect < 6; redirect++) {
            checkCancelled();
            String host = url.getHost().toLowerCase(Locale.ROOT);
            if (!"https".equals(url.getProtocol()) || url.getUserInfo() != null ||
                    (url.getPort() != -1 && url.getPort() != 443) ||
                    !(host.equals("gcode.bytarch.com") || host.equals("github.com") || host.endsWith(".githubusercontent.com"))) {
                throw new IOException("Download redirected outside the allowed HTTPS manifest/release hosts.");
            }
            HttpURLConnection connection = connections.open(url);
            activeConnection = connection;
            connection.setInstanceFollowRedirects(false);
            connection.setConnectTimeout(20000); connection.setReadTimeout(30000);
            connection.setUseCaches(false);
            connection.setRequestProperty("User-Agent", "GTA5-Android-Muguet/0.1.1");
            connection.setRequestProperty("Accept-Encoding", "identity");
            if (offset > 0) connection.setRequestProperty("Range", "bytes=" + offset + "-");
            int code;
            try { code = connection.getResponseCode(); }
            catch (IOException error) { connection.disconnect(); activeConnection = null; throw error; }
            if (code == 301 || code == 302 || code == 303 || code == 307 || code == 308) {
                String location = connection.getHeaderField("Location");
                connection.disconnect();
                if (location == null) throw new IOException("Missing release redirect location.");
                url = new URL(url, location);
            } else return connection;
        }
        throw new IOException("Too many release redirects.");
    }
    private String fetchManifest() throws Exception {
        HttpURLConnection connection = null;
        try {
            connection = open(MANIFEST_URL, 0);
            if (connection.getResponseCode() != 200) throw new IOException("Manifest request failed: HTTP " + connection.getResponseCode());
            try (InputStream input = connection.getInputStream(); ByteArrayOutputStream output = new ByteArrayOutputStream()) {
                byte[] buffer = new byte[8192]; int n;
                while ((n = input.read(buffer)) != -1) {
                    checkCancelled();
                    if (output.size() + n > 1 << 20) throw new IOException("Release manifest is too large.");
                    output.write(buffer, 0, n);
                }
                return output.toString(StandardCharsets.UTF_8.name());
            }
        } finally { if (connection != null) connection.disconnect(); activeConnection = null; }
    }
    void install() throws Exception {
        Files.createDirectories(work.toPath());
        // A durable commit marker is written only after all extracted files have been verified.
        if (new File(work, "committing").isFile()) { commit(); return; }
        update("Fetching release manifest…", 0, 0, true);
        String text = fetchManifest();
        Manifest manifest = new Manifest(text);
        File snapshot = new File(work, "manifest.json");
        if (snapshot.isFile() && !new Manifest(readText(snapshot, 1 << 20)).fingerprint.equals(manifest.fingerprint)) {
            throw new IOException("The release changed. Discard the partial download before installing the new release.");
        }
        if (!snapshot.isFile()) writeAtomic(snapshot, text);
        Files.createDirectories(stage.toPath());
        Map<String, JSONObject> records = new LinkedHashMap<>();
        long extracted = 0, transferred = 0;
        int completed = 0;
        for (int i = 0; i < manifest.parts.size(); i++) {
            File receipt = receipt(i);
            if (!receipt.isFile()) break;
            Part part = manifest.parts.get(i);
            JSONObject json = new JSONObject(readText(receipt, 8 << 20));
            if (!part.sha256.equals(json.getString("sha256"))) throw new IOException("Partial installation belongs to another release.");
            JSONArray entries = json.getJSONArray("files");
            for (int k = 0; k < entries.length(); k++) {
                JSONObject entry = entries.getJSONObject(k);
                String relative = entry.getString("path");
                File file = safeFile(stage, relative);
                if (selected(relative) == null || records.put(relative, entry) != null || !file.isFile() || file.length() != entry.getLong("bytes")) {
                    throw new IOException("Partial installation is incomplete or damaged. Discard it and restart.");
                }
                extracted += file.length();
                if (records.size() > MAX_FILES || extracted > MAX_GAME_BYTES) throw new IOException("Partial installation exceeds size limits.");
            }
            transferred += part.bytes; completed++;
            Files.deleteIfExists(new File(work, part.name).toPath());
        }
        // Allow one archive at a time, extracted assets, and spare space for native-code caches.
        long required = Math.max(RESERVE, manifest.bytes + manifest.largest + RESERVE - extracted);
        File current = completed < manifest.parts.size() ? new File(work, manifest.parts.get(completed).name) : null;
        if (current != null) required = Math.max(RESERVE, required - current.length());
        requireSpace(required);
        for (int i = completed; i < manifest.parts.size(); i++) {
            checkCancelled();
            Part part = manifest.parts.get(i);
            File archive = new File(work, part.name);
            download(part, archive, transferred, manifest.bytes, i + 1, manifest.parts.size());
            JSONArray entries = extract(archive, records, extracted, i + 1, manifest.parts.size());
            // Receipt is persisted before deleting the archive; retries cannot mistake a partial extraction for a complete part.
            JSONObject receipt = new JSONObject().put("sha256", part.sha256).put("files", entries);
            writeAtomic(receipt(i), receipt.toString());
            for (int k = 0; k < entries.length(); k++) {
                JSONObject entry = entries.getJSONObject(k);
                records.put(entry.getString("path"), entry); extracted += entry.getLong("bytes");
            }
            Files.delete(archive.toPath());
            transferred += part.bytes;
        }
        validateGame(stage);
        long checked = 0;
        update("Verifying installed game files…", 0, extracted, true);
        for (JSONObject entry : records.values()) {
            File file = safeFile(stage, entry.getString("path"));
            if (file.length() != entry.getLong("bytes") || !hash(file, "Verifying installed game files…", checked, extracted).equals(entry.getString("sha256"))) {
                throw new IOException("Extracted game file failed verification. Discard the partial download and restart.");
            }
            checked += file.length();
        }
        checkCancelled();
        writeAtomic(new File(work, "committing"), manifest.fingerprint);
        commit();
    }
    private File receipt(int index) { return new File(work, String.format(Locale.ROOT, "part-%02d.json", index + 1)); }
    private void requireSpace(long needed) throws IOException {
        long available = files.getUsableSpace();
        if (available < needed) throw new IOException(String.format(Locale.ROOT,
            "Not enough internal storage: need %.1f GB free, have %.1f GB. Existing game files are preserved.", needed / 1e9, available / 1e9));
    }
    private void download(Part part, File archive, long previous, long total, int index, int count) throws Exception {
        String label = "Downloading part " + index + "/" + count;
        for (int attempt = 0; attempt < 4; attempt++) {
            checkCancelled();
            long offset = archive.length();
            if (offset > part.bytes) { Files.delete(archive.toPath()); offset = 0; }
            HttpURLConnection connection = null;
            try {
                if (offset < part.bytes) {
                    requireSpace(part.bytes - offset + GIB);
                    update(label, previous + offset, total, true);
                    connection = open(part.url, offset);
                    offset = responseOffset(connection, offset, part.bytes, index);
                    try (InputStream input = connection.getInputStream(); FileOutputStream output = new FileOutputStream(archive, offset > 0)) {
                        int n;
                        while ((n = input.read(buffer)) != -1) {
                            checkCancelled();
                            if (offset + n > part.bytes) throw new IOException("Archive exceeds its declared size.");
                            output.write(buffer, 0, n); offset += n;
                            update(label, previous + offset, total, false);
                        }
                        output.getFD().sync();
                    }
                    if (offset != part.bytes) throw new EOFException("Connection ended before part " + index + " finished.");
                }
                String digest = hash(archive, "Checking SHA-256 for part " + index + "/" + count, 0, part.bytes);
                if (!digest.equals(part.sha256)) {
                    Files.delete(archive.toPath());
                    throw new IntegrityException("SHA-256 mismatch for part " + index + ". The bad archive was removed; resume to retry.");
                }
                return;
            } catch (IOException error) {
                checkCancelled();
                if (attempt == 3 || error instanceof IntegrityException) throw error;
                update("Connection/checksum error; retrying part " + index + "…", 0, 0, true);
                for (int k = 0; k < (attempt + 1) * 10; k++) { checkCancelled(); Thread.sleep(500); }
            } finally { if (connection != null) connection.disconnect(); activeConnection = null; }
        }
    }
    private static final class IntegrityException extends IOException {
        IntegrityException(String message) { super(message); }
    }
    static long responseOffset(HttpURLConnection connection, long offset, long bytes, int index) throws IOException {
        int code = connection.getResponseCode();
        if (code == 200) offset = 0; // An ignored Range restarts the file; never append a full response.
        else if (offset > 0 && code == 206) {
            String expected = "bytes " + offset + "-" + (bytes - 1) + "/" + bytes;
            if (!expected.equals(connection.getHeaderField("Content-Range"))) throw new IOException("Invalid resume response for part " + index);
        } else throw new IOException("Part " + index + " request failed: HTTP " + code);
        String encoding = connection.getHeaderField("Content-Encoding");
        if (encoding != null && !"identity".equalsIgnoreCase(encoding)) throw new IOException("Unexpected archive content encoding.");
        long length = connection.getContentLengthLong();
        if (length >= 0 && length != bytes - offset) throw new IOException("Archive response has an unexpected size.");
        return offset;
    }
    private String hash(File file, String label, long previous, long total) throws Exception {
        MessageDigest digest = MessageDigest.getInstance("SHA-256");
        long done = 0;
        try (InputStream input = new FileInputStream(file)) {
            int n;
            while ((n = input.read(buffer)) != -1) {
                checkCancelled(); digest.update(buffer, 0, n); done += n;
                update(label, previous + done, total, false);
            }
        }
        return hex(digest.digest());
    }
    // The release uses a fixed mirror prefix. Only runtime assets are installed, never browser JS or Windows helpers.
    static String selected(String relative) throws IOException {
        if (relative.endsWith("/") || !relative.startsWith("data/") && !relative.startsWith("b/")) return null;
        for (String segment : relative.split("/", -1)) {
            if (segment.isEmpty() || segment.equals(".") || segment.equals("..") || segment.indexOf('\\') >= 0 || segment.indexOf(':') >= 0) {
                throw new IOException("Unsafe game archive path.");
            }
            if (segment.equalsIgnoreCase("env_test") || segment.equalsIgnoreCase("verityradio")) return null;
        }
        if (relative.startsWith("b/")) {
            int slash = relative.indexOf('/', 2);
            String inner = slash < 0 ? "" : relative.substring(slash + 1);
            if (!inner.equals("game.wasm") && !inner.startsWith("shaders/")) return null;
        }
        return relative;
    }
    private static File safeFile(File root, String relative) throws IOException {
        if (selected(relative) == null) throw new IOException("Invalid staged game path.");
        File file = new File(root, relative);
        if (!file.getCanonicalPath().startsWith(root.getCanonicalPath() + File.separator)) throw new IOException("Unsafe game archive path.");
        return file;
    }
    private JSONArray extract(File archive, Map<String, JSONObject> previous, long installed, int index, int count) throws Exception {
        JSONArray records = new JSONArray();
        try (ZipFile zip = new ZipFile(archive)) {
            List<ZipEntry> jobs = new ArrayList<>(); Set<String> names = new HashSet<>();
            long total = 0;
            Enumeration<? extends ZipEntry> entries = zip.entries();
            while (entries.hasMoreElements()) {
                checkCancelled();
                ZipEntry entry = entries.nextElement();
                if (!entry.getName().startsWith(PREFIX)) continue;
                String relative = selected(entry.getName().substring(PREFIX.length()));
                if (relative == null) continue;
                safeFile(stage, relative);
                if (!names.add(relative) || previous.containsKey(relative)) throw new IOException("Duplicate game file across release parts.");
                if (entry.getSize() < 0 || entry.getSize() > 4 * GIB || entry.getCrc() < 0) throw new IOException("Invalid game entry size/checksum.");
                total += entry.getSize(); jobs.add(entry);
                if (jobs.size() + previous.size() > MAX_FILES || installed + total > MAX_GAME_BYTES) throw new IOException("Game archive exceeds extraction limits.");
            }
            if (jobs.isEmpty()) throw new IOException("Release part has no compatible game assets.");
            requireSpace(total + GIB);
            long done = 0;
            String label = "Installing part " + index + "/" + count;
            update(label, 0, total, true);
            File temporary = new File(work, "entry.tmp");
            for (ZipEntry entry : jobs) {
                checkCancelled();
                String relative = selected(entry.getName().substring(PREFIX.length()));
                File target = safeFile(stage, relative);
                Files.createDirectories(target.getParentFile().toPath());
                MessageDigest digest = MessageDigest.getInstance("SHA-256"); CRC32 crc = new CRC32();
                long bytes = 0;
                try (InputStream input = zip.getInputStream(entry); FileOutputStream output = new FileOutputStream(temporary)) {
                    int n;
                    while ((n = input.read(buffer)) != -1) {
                        checkCancelled();
                        if (bytes + n > entry.getSize()) throw new IOException("Game entry exceeds its declared size.");
                        output.write(buffer, 0, n); digest.update(buffer, 0, n); crc.update(buffer, 0, n);
                        bytes += n; done += n; update(label, done, total, false);
                    }
                    output.getFD().sync();
                } catch (Exception error) { Files.deleteIfExists(temporary.toPath()); throw error; }
                if (bytes != entry.getSize() || crc.getValue() != entry.getCrc()) throw new IOException("Game entry failed ZIP checksum verification.");
                Files.move(temporary.toPath(), target.toPath(), StandardCopyOption.REPLACE_EXISTING);
                records.put(new JSONObject().put("path", relative).put("bytes", bytes).put("sha256", hex(digest.digest())));
            }
        }
        return records;
    }
    private static void validateGame(File root) throws IOException {
        if (!new File(root, "data/manifest.json").isFile()) throw new IOException("Missing game data manifest.");
        File[] builds = new File(root, "b").listFiles();
        if (builds != null) for (File build : builds) {
            if (new File(build, "game.wasm").isFile() && new File(build, "shaders/index.json").isFile()) return;
        }
        throw new IOException("Missing matching game.wasm and shader index.");
    }
    private static void tidyLists(File root) throws Exception {
        File data = new File(root, "data"), dlc = new File(data, "common/data/dlcList.xml");
        if (dlc.isFile()) {
            List<String> lines = Files.readAllLines(dlc.toPath(), StandardCharsets.UTF_8);
            lines.removeIf(line -> line.toLowerCase(Locale.ROOT).contains("env_test") || line.toLowerCase(Locale.ROOT).contains("verityradio"));
            writeAtomic(dlc, String.join("\n", lines) + "\n");
        }
        File path = new File(data, "manifest.json");
        JSONObject manifest = new JSONObject(readText(path, 8 << 20));
        JSONArray entries = manifest.optJSONArray("files");
        if (entries != null) {
            JSONArray kept = new JSONArray();
            for (int i = 0; i < entries.length(); i++) {
                JSONArray entry = entries.optJSONArray(i);
                String name = entry == null ? "" : entry.optString(0, "");
                if (name.isEmpty()) continue;
                File file = new File(data, name);
                if (file.getCanonicalPath().startsWith(data.getCanonicalPath() + File.separator) && file.isFile()) kept.put(entry);
            }
            manifest.put("files", kept);
        }
        writeAtomic(path, manifest.toString());
    }
    private void commit() throws Exception {
        // Do not honor pause between renames. A crash can recover from this journal on the next Resume.
        update("Finishing installation—please wait…", 0, 0, true);
        File game = new File(files, "game"), backup = new File(work, "previous-game");
        if (stage.isDirectory()) {
            validateGame(stage); tidyLists(stage);
            if (game.exists()) {
                if (backup.exists()) throw new IOException("Installation recovery found conflicting game directories.");
                Files.move(game.toPath(), backup.toPath());
            }
            try { Files.move(stage.toPath(), game.toPath()); }
            catch (IOException error) {
                if (!game.exists() && backup.exists()) Files.move(backup.toPath(), game.toPath());
                throw error;
            }
        }
        validateGame(game);
        // Delete the commit marker last so interrupted cleanup remains recoverable.
        File[] leftovers = work.listFiles();
        if (leftovers != null) for (File file : leftovers) if (!file.getName().equals("committing")) deleteTree(file);
        Files.delete(new File(work, "committing").toPath());
        Files.delete(work.toPath());
        update("Game download and installation complete.", 1, 1, true);
    }
    static boolean hasPartial(File files) { return new File(files, "game-download").isDirectory(); }
    static boolean needsRecovery(File files) { return new File(files, "game-download/committing").exists(); }
    static void discardPartial(File files) throws IOException {
        File work = new File(files, "game-download");
        if (new File(work, "committing").exists() || new File(work, "previous-game").exists()) throw new IOException("Installation needs recovery. Tap Download / Resume first.");
        deleteTree(work);
    }
    private static void deleteTree(File root) throws IOException {
        if (!root.exists()) return;
        // App-private installer paths only; never follow symlinks into another directory.
        try (java.util.stream.Stream<Path> paths = Files.walk(root.toPath())) {
            for (Path path : (Iterable<Path>) paths.sorted(Comparator.reverseOrder())::iterator) Files.delete(path);
        }
    }
    private static String readText(File file, int limit) throws IOException {
        if (file.length() > limit) throw new IOException("Installation metadata exceeds size limit.");
        return new String(Files.readAllBytes(file.toPath()), StandardCharsets.UTF_8);
    }
    private static void writeAtomic(File target, String text) throws IOException {
        File temporary = new File(target.getParentFile(), target.getName() + ".new");
        try (FileOutputStream output = new FileOutputStream(temporary)) {
            output.write(text.getBytes(StandardCharsets.UTF_8)); output.getFD().sync();
        }
        try { Files.move(temporary.toPath(), target.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING); }
        catch (AtomicMoveNotSupportedException error) { Files.move(temporary.toPath(), target.toPath(), StandardCopyOption.REPLACE_EXISTING); }
    }
}
