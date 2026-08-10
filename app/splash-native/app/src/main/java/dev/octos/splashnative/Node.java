package dev.octos.splashnative;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/** Decoded Splash node. Mirrors the encoder in rust/src/lib.rs. */
public final class Node {
    public int id;
    public String kind;
    public final Map<String, Object> attrs = new HashMap<>();
    public final List<Node> children = new ArrayList<>();

    public String s(String k) { Object v = attrs.get(k); return v instanceof String ? (String) v : null; }
    public String s(String k, String d) { String v = s(k); return v == null ? d : v; }
    public Double n(String k) { Object v = attrs.get(k); return v instanceof Double ? (Double) v : null; }
    public float f(String k, float d) { Double v = n(k); return v == null ? d : v.floatValue(); }
    public int i(String k, int d) { Double v = n(k); return v == null ? d : (int) Math.round(v); }
    public boolean b(String k, boolean d) { Double v = n(k); return v == null ? d : v != 0.0; }
    public boolean has(String k) { return attrs.containsKey(k); }

    static final int MAGIC = 0x53504332;
    static final int T_F64 = 0, T_STR = 1;

    /** Decode the whole tree; returns the root, or null. */
    public static Node decode(ByteBuffer bb) {
        bb.order(ByteOrder.LITTLE_ENDIAN);
        bb.position(0);
        int magic = bb.getInt();
        if (magic != MAGIC) throw new IllegalStateException("bad magic " + Integer.toHexString(magic));
        int count = bb.getInt();
        int blobLen = bb.getInt();

        int blobStart = bb.limit() - blobLen;
        byte[] blob = new byte[blobLen];
        int save = bb.position();
        bb.position(blobStart);
        bb.get(blob);
        bb.position(save);

        Node root = null;
        Map<Integer, Node> byId = new HashMap<>();
        for (int k = 0; k < count; k++) {
            int id = bb.getInt();
            int parent = bb.getInt();
            int ko = bb.getInt(), kl = bb.getInt();
            int na = bb.getInt();

            Node n = new Node();
            n.id = id;
            n.kind = str(blob, ko, kl);
            for (int j = 0; j < na; j++) {
                int no = bb.getInt(), nl = bb.getInt();
                int ty = bb.get() & 0xFF;
                bb.get(); bb.get(); bb.get();
                String name = str(blob, no, nl);
                if (ty == T_F64) {
                    long bits = bb.getLong();
                    n.attrs.put(name, Double.longBitsToDouble(bits));
                } else {
                    int o = bb.getInt(), l = bb.getInt();
                    n.attrs.put(name, str(blob, o, l));
                }
            }
            byId.put(id, n);
            if (parent == -1) root = n;
            else {
                Node p = byId.get(parent);
                if (p != null) p.children.add(n);
            }
        }
        return root;
    }

    static String str(byte[] blob, int off, int len) {
        if (off < 0 || len < 0 || off + len > blob.length) return "";
        try { return new String(blob, off, len, "UTF-8"); } catch (Exception e) { return ""; }
    }
}
