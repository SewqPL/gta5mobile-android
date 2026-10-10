package dev.muguet;

import org.json.JSONArray;
import org.json.JSONObject;
import org.junit.Rule;
import org.junit.Test;
import org.junit.rules.TemporaryFolder;
import java.io.*;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.security.MessageDigest;
import java.util.*;
import java.util.zip.ZipEntry;
import java.util.zip.ZipOutputStream;
import static org.junit.Assert.*;

public class GameInstallerTest {
    @Rule public TemporaryFolder temporary = new TemporaryFolder();
    private static final String PREFIX = "mirror/playgta5.com/";
    private static String sha(byte[] data) throws Exception {
        StringBuilder text = new StringBuilder();
        for (byte b : MessageDigest.getInstance("SHA-256").digest(data)) text.append(String.format(Locale.ROOT, "%02x", b & 255));
        return text.toString();
    }
    private static void writeText(File file, String text) throws Exception {
        Files.write(file.toPath(), text.getBytes(StandardCharsets.UTF_8));
    }
    private static String readText(File file) throws Exception {
        return new String(Files.readAllBytes(file.toPath()), StandardCharsets.UTF_8);
    }
    private static byte[] zip(Map<String, byte[]> entries) throws Exception {
        ByteArrayOutputStream bytes = new ByteArrayOutputStream();
        try (ZipOutputStream zip = new ZipOutputStream(bytes)) {
            for (Map.Entry<String, byte[]> entry : entries.entrySet()) {
                zip.putNextEntry(new ZipEntry(entry.getKey())); zip.write(entry.getValue()); zip.closeEntry();
            }
        }
        return bytes.toByteArray();
    }
    private static Map<String, byte[]> firstEntries() {
        Map<String, byte[]> entries = new LinkedHashMap<>();
        entries.put(PREFIX + "data/manifest.json", "{\"files\":[[\"asset.bin\",5],[\"dlcpacks/env_test/file.bin\",4]]}".getBytes(StandardCharsets.UTF_8));
        entries.put(PREFIX + "b/build/game.wasm", new byte[]{0, 97, 115, 109, 1, 0, 0, 0});
        entries.put(PREFIX + "b/build/shaders/index.json", "{}".getBytes(StandardCharsets.UTF_8));
        entries.put(PREFIX + "b/build/worker.js", new byte[]{1});
        entries.put(PREFIX + "data/dlcpacks/env_test/file.bin", new byte[]{1, 2, 3, 4});
        entries.put("runtime/python.exe", new byte[]{1});
        return entries;
    }
    private static JSONObject manifest(List<byte[]> archives) throws Exception {
        JSONArray parts = new JSONArray();
        for (int i = 0; i < archives.size(); i++) {
            String name = String.format(Locale.ROOT, "playgta5-part-%02d-of-%02d.zip", i + 1, archives.size());
            byte[] archive = archives.get(i);
            parts.put(new JSONObject().put("name", name).put("bytes", archive.length).put("sha256", sha(archive)).put("url", GameInstaller.RELEASE + name));
        }
        return new JSONObject().put("version", 1).put("repository", "alihammoud21/playgta5").put("tag", "full-game-v1").put("parts", parts);
    }
    private static class Response extends HttpURLConnection {
        final byte[] body;
        final int code;
        final Map<String, String> headers = new HashMap<>();
        Response(URL url, byte[] body, int code) { super(url); this.body = body; this.code = code; }
        @Override public void connect() {}
        @Override public void disconnect() {}
        @Override public boolean usingProxy() { return false; }
        @Override public int getResponseCode() { return code; }
        @Override public InputStream getInputStream() { return new ByteArrayInputStream(body); }
        @Override public long getContentLengthLong() { return body.length; }
        @Override public String getHeaderField(String key) { return headers.get(key); }
    }
    private static final class Server implements GameInstaller.Connections {
        final JSONObject manifest;
        final List<byte[]> archives;
        final Map<String, Integer> requests = new HashMap<>();
        final List<Response> responses = new ArrayList<>();
        boolean ignoreRange;
        Server(List<byte[]> archives) throws Exception { this.archives = archives; this.manifest = manifest(archives); }
        @Override public HttpURLConnection open(URL url) throws IOException {
            String address = url.toString();
            requests.merge(address, 1, Integer::sum);
            if (address.equals(GameInstaller.MANIFEST_URL)) return new Response(url, manifest.toString().getBytes(StandardCharsets.UTF_8), 200);
            try {
                JSONArray parts = manifest.getJSONArray("parts");
                for (int i = 0; i < parts.length(); i++) if (address.equals(parts.getJSONObject(i).getString("url"))) {
                    byte[] data = archives.get(i);
                    Response response = new Response(url, data, 200) {
                        private int offset() {
                            String range = getRequestProperty("Range");
                            return range == null || ignoreRange ? 0 : Integer.parseInt(range.substring(6, range.length() - 1));
                        }
                        @Override public int getResponseCode() { return offset() > 0 ? 206 : 200; }
                        @Override public InputStream getInputStream() { return new ByteArrayInputStream(data, offset(), data.length - offset()); }
                        @Override public long getContentLengthLong() { return data.length - offset(); }
                        @Override public String getHeaderField(String key) {
                            return key.equals("Content-Range") && offset() > 0 ? "bytes " + offset() + "-" + (data.length - 1) + "/" + data.length : null;
                        }
                    };
                    responses.add(response); return response;
                }
                throw new IOException("Unexpected URL: " + address);
            } catch (Exception error) { throw new IOException(error); }
        }
    }
    private Server server() throws Exception {
        return new Server(Arrays.asList(zip(firstEntries()), zip(Collections.singletonMap(PREFIX + "data/asset.bin", "asset".getBytes(StandardCharsets.UTF_8)))));
    }
    private GameInstaller installer(File root, Server server) { return new GameInstaller(root, (text, done, total) -> {}, server); }
    @Test public void manifestValidatesSchemaAndDownloadOrigin() throws Exception {
        Server server = server();
        GameInstaller.Manifest value = new GameInstaller.Manifest(server.manifest.toString());
        assertEquals(2, value.parts.size());
        assertEquals(server.archives.get(0).length + server.archives.get(1).length, value.bytes);
        server.manifest.getJSONArray("parts").getJSONObject(0).put("url", "https://evil.example/game.zip");
        assertThrows(IOException.class, () -> new GameInstaller.Manifest(server.manifest.toString()));
    }
    @Test public void numericSizeAndHashMustBeValid() throws Exception {
        Server server = server();
        JSONObject part = server.manifest.getJSONArray("parts").getJSONObject(0);
        part.put("bytes", 0); assertThrows(IOException.class, () -> new GameInstaller.Manifest(server.manifest.toString()));
        part.put("bytes", "100"); assertThrows(IOException.class, () -> new GameInstaller.Manifest(server.manifest.toString()));
        part.put("bytes", 0.5); assertThrows(IOException.class, () -> new GameInstaller.Manifest(server.manifest.toString()));
        part.put("bytes", 100).put("sha256", "bad"); assertThrows(IOException.class, () -> new GameInstaller.Manifest(server.manifest.toString()));
    }
    @Test public void usesRequestedManifestEndpointAndRejectsInsecureRedirects() throws Exception {
        assertEquals("https://gcode.bytarch.com/download-manifest.json", GameInstaller.MANIFEST_URL);
        File root = temporary.newFolder(); List<String> requests = new ArrayList<>();
        GameInstaller value = new GameInstaller(root, (text, done, total) -> {}, url -> {
            requests.add(url.toString());
            Response redirect = new Response(url, new byte[0], 302);
            redirect.headers.put("Location", "http://gcode.bytarch.com/download-manifest.json");
            return redirect;
        });
        assertThrows(IOException.class, value::install);
        assertEquals(Collections.singletonList(GameInstaller.MANIFEST_URL), requests);
    }
    @Test public void lowDiskSpaceStopsBeforeDownloadingArchives() throws Exception {
        File directory = temporary.newFolder();
        File root = new File(directory.getPath()) { @Override public long getUsableSpace() { return 0; } };
        Server server = server();
        assertThrows(IOException.class, () -> installer(root, server).install());
        assertEquals(Collections.singleton(GameInstaller.MANIFEST_URL), server.requests.keySet());
        assertFalse(new File(root, "game").exists());
    }
    @Test public void installsAllPartsWithoutWindowsHelpersOrCombinedZip() throws Exception {
        File root = temporary.newFolder(); Server server = server();
        installer(root, server).install();
        assertArrayEquals("asset".getBytes(StandardCharsets.UTF_8), Files.readAllBytes(new File(root, "game/data/asset.bin").toPath()));
        assertTrue(new File(root, "game/b/build/game.wasm").isFile());
        assertFalse(new File(root, "game/runtime").exists());
        assertFalse(new File(root, "game/b/build/worker.js").exists());
        assertFalse(new File(root, "game/data/dlcpacks/env_test").exists());
        assertFalse(GameInstaller.hasPartial(root));
        JSONObject data = new JSONObject(readText(new File(root, "game/data/manifest.json")));
        assertEquals(1, data.getJSONArray("files").length());
    }
    @Test public void pausesBetweenPartsAndResumesWithoutRedownloadingCompletedParts() throws Exception {
        File root = temporary.newFolder(); Server server = server(); GameInstaller[] active = new GameInstaller[1];
        active[0] = new GameInstaller(root, (text, done, total) -> { if (text.equals("Downloading part 2/2")) active[0].cancel(); }, server);
        assertThrows(InterruptedIOException.class, () -> active[0].install());
        assertTrue(new File(root, "game-download/part-01.json").isFile());
        assertFalse(new File(root, "game").exists());
        installer(root, server).install();
        String part = server.manifest.getJSONArray("parts").getJSONObject(0).getString("url");
        assertEquals(Integer.valueOf(1), server.requests.get(part));
        assertFalse(GameInstaller.hasPartial(root));
    }
    @Test public void resumesPartialArchiveUsingRange() throws Exception {
        resume(false);
    }
    @Test public void damagedCompletedStageCannotReplaceOldGame() throws Exception {
        File root = temporary.newFolder(); Server server = server(); GameInstaller[] active = new GameInstaller[1];
        File old = new File(root, "game/keep"); Files.createDirectories(old.getParentFile().toPath()); writeText(old, "old game");
        active[0] = new GameInstaller(root, (text, done, total) -> { if (text.equals("Downloading part 2/2")) active[0].cancel(); }, server);
        assertThrows(InterruptedIOException.class, () -> active[0].install());
        File code = new File(root, "game-download/game/b/build/game.wasm");
        Files.write(code.toPath(), new byte[(int) code.length()]); // Same size, different hash.
        assertThrows(IOException.class, () -> installer(root, server).install());
        assertEquals("old game", readText(old));
        assertFalse(GameInstaller.needsRecovery(root));
    }
    @Test public void extractionPauseCanRetrySameArchiveSafely() throws Exception {
        File root = temporary.newFolder(); Server server = server(); GameInstaller[] active = new GameInstaller[1];
        active[0] = new GameInstaller(root, (text, done, total) -> { if (text.equals("Installing part 1/2")) active[0].cancel(); }, server);
        assertThrows(InterruptedIOException.class, () -> active[0].install());
        assertFalse(new File(root, "game-download/part-01.json").exists());
        installer(root, server).install();
        assertTrue(new File(root, "game/data/asset.bin").isFile());
        String part = server.manifest.getJSONArray("parts").getJSONObject(0).getString("url");
        assertEquals(Integer.valueOf(1), server.requests.get(part));
    }
    @Test public void ignoredRangeRestartsFileWithoutAppending() throws Exception {
        resume(true);
    }
    private void resume(boolean ignoreRange) throws Exception {
        File root = temporary.newFolder(); Server server = server(); server.ignoreRange = ignoreRange;
        File work = new File(root, "game-download"); assertTrue(work.mkdir());
        JSONObject part = server.manifest.getJSONArray("parts").getJSONObject(0);
        Files.write(new File(work, part.getString("name")).toPath(), Arrays.copyOf(server.archives.get(0), 100));
        installer(root, server).install();
        assertEquals("bytes=100-", server.responses.get(0).getRequestProperty("Range"));
        assertTrue(new File(root, "game/data/asset.bin").isFile());
    }
    @Test public void rejectsMalformedRangeAndUnexpectedResponseSize() throws Exception {
        Response response = new Response(new URL(GameInstaller.RELEASE + "test.zip"), new byte[50], 206);
        response.headers.put("Content-Range", "bytes 50-99/100");
        assertEquals(50, GameInstaller.responseOffset(response, 50, 100, 1));
        response.headers.put("Content-Range", "bytes 0-49/100");
        assertThrows(IOException.class, () -> GameInstaller.responseOffset(response, 50, 100, 1));
        assertThrows(IOException.class, () -> GameInstaller.responseOffset(new Response(response.getURL(), new byte[99], 200), 50, 100, 1));
    }
    @Test public void checksumMismatchDoesNotReplaceInstalledGame() throws Exception {
        File root = temporary.newFolder(); Server server = server();
        File old = new File(root, "game/old-save-marker"); Files.createDirectories(old.getParentFile().toPath()); writeText(old, "keep");
        server.manifest.getJSONArray("parts").getJSONObject(0).put("sha256", "0".repeat(64));
        assertThrows(IOException.class, () -> installer(root, server).install());
        assertEquals("keep", readText(old));
        assertFalse(new File(root, "game-download/part-01.json").exists());
        assertFalse(new File(root, "game-download/" + server.manifest.getJSONArray("parts").getJSONObject(0).getString("name")).exists());
    }
    @Test public void traversalAndDuplicatePathsAreRejected() throws Exception {
        for (String path : Arrays.asList("data/../escape", "data/\\escape", "data/C:/escape", "data//escape")) {
            assertThrows(IOException.class, () -> GameInstaller.selected(path));
        }
        File root = temporary.newFolder(); Server duplicate = new Server(Arrays.asList(zip(firstEntries()), zip(Collections.singletonMap(PREFIX + "data/manifest.json", new byte[]{1}))));
        assertThrows(IOException.class, () -> installer(root, duplicate).install());
        assertFalse(new File(root, "game").exists());
    }
    @Test public void missingGameCodeCannotCommit() throws Exception {
        File root = temporary.newFolder(); Map<String, byte[]> entries = firstEntries(); entries.remove(PREFIX + "b/build/game.wasm");
        Server server = new Server(Collections.singletonList(zip(entries)));
        assertThrows(IOException.class, () -> installer(root, server).install());
        assertFalse(new File(root, "game").exists());
    }
    @Test public void changedManifestRequiresDiscardAndPreservesInstalledGame() throws Exception {
        File root = temporary.newFolder(); Server server = server();
        File work = new File(root, "game-download"); assertTrue(work.mkdir());
        writeText(new File(work, "manifest.json"), server.manifest.toString());
        server.manifest.getJSONArray("parts").getJSONObject(0).put("sha256", "0".repeat(64));
        assertThrows(IOException.class, () -> installer(root, server).install());
        assertEquals(Integer.valueOf(1), server.requests.get(GameInstaller.MANIFEST_URL));
        assertEquals(1, server.requests.size());
        GameInstaller.discardPartial(root); assertFalse(work.exists());
    }
    @Test public void commitRecoveryNeedsNoNetworkAndPreservesSaves() throws Exception {
        File root = temporary.newFolder(); File work = new File(root, "game-download");
        File stage = new File(work, "game");
        Files.createDirectories(new File(stage, "data").toPath());
        Files.createDirectories(new File(stage, "b/build/shaders").toPath());
        writeText(new File(stage, "data/manifest.json"), "{\"files\":[]}");
        Files.write(new File(stage, "b/build/game.wasm").toPath(), new byte[]{0});
        writeText(new File(stage, "b/build/shaders/index.json"), "{}");
        writeText(new File(work, "committing"), "verified");
        Files.createDirectories(new File(work, "previous-game").toPath());
        writeText(new File(root, "config.json"), "keep settings");
        assertTrue(GameInstaller.needsRecovery(root));
        assertThrows(IOException.class, () -> GameInstaller.discardPartial(root));
        new GameInstaller(root, (text, done, total) -> {}, url -> { throw new IOException("No network allowed"); }).install();
        assertFalse(work.exists()); assertTrue(new File(root, "game/b/build/game.wasm").exists());
        assertEquals("keep settings", readText(new File(root, "config.json")));
    }
}
