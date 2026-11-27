
#define RS "RootConstants(num32BitConstants=6, b0), SRV(t0), UAV(u0), UAV(u1)"
cbuffer Span : register(b0) {
    uint first;    // the first piece
    uint skew;     // where the copy starts in its piece 0
    uint size;     // the copy's buffer, in bytes (a multiple of 64 KiB)
    uint pieces;
    uint out_at;   // the first piece's dword in `differs`, in bytes
    uint compare;  // 0: the hashes are stored; else compared with those stored (2: and then stored)
};
ByteAddressBuffer now : register(t0);
// 16 bytes per piece of the copy: the hash of its bytes when they were
// last sent up (or hashed whole, the first time).
RWByteAddressBuffer hashes : register(u0);
RWByteAddressBuffer differs : register(u1);
groupshared uint4 parts[64];
[RootSignature(RS)]
[numthreads(64, 1, 1)]
void main(uint3 id : SV_GroupID, uint thread : SV_GroupIndex) {
    uint piece = first + id.x;
    uint lo = piece * 65536u;
    lo = lo > skew ? lo - skew : 0u;
    uint hi = min((piece + 1u) * 65536u - skew, size);
    lo = lo & ~15u;
    hi = (hi + 15u) & ~15u;
    uint4 h = uint4(0x9E3779B9u, 0x85EBCA6Bu, 0xC2B2AE35u, 0x27D4EB2Fu) + thread;
    uint at = lo + thread * 16u;
    // Eight loads at once: one after the other, each waits for memory.
    [loop]
    for (; at + 7168u < hi; at += 8192u) {
        uint4 v0 = now.Load4(at + 0u);
        uint4 v1 = now.Load4(at + 1024u);
        uint4 v2 = now.Load4(at + 2048u);
        uint4 v3 = now.Load4(at + 3072u);
        uint4 v4 = now.Load4(at + 4096u);
        uint4 v5 = now.Load4(at + 5120u);
        uint4 v6 = now.Load4(at + 6144u);
        uint4 v7 = now.Load4(at + 7168u);
        h = (h ^ v0) * uint4(0x01000193u, 0x9E3779B1u, 0x85EBCA77u, 0xC2B2AE3Du);
        h = h.yzwx ^ (h >> 15);
        h = (h ^ v1) * uint4(0x01000193u, 0x9E3779B1u, 0x85EBCA77u, 0xC2B2AE3Du);
        h = h.yzwx ^ (h >> 15);
        h = (h ^ v2) * uint4(0x01000193u, 0x9E3779B1u, 0x85EBCA77u, 0xC2B2AE3Du);
        h = h.yzwx ^ (h >> 15);
        h = (h ^ v3) * uint4(0x01000193u, 0x9E3779B1u, 0x85EBCA77u, 0xC2B2AE3Du);
        h = h.yzwx ^ (h >> 15);
        h = (h ^ v4) * uint4(0x01000193u, 0x9E3779B1u, 0x85EBCA77u, 0xC2B2AE3Du);
        h = h.yzwx ^ (h >> 15);
        h = (h ^ v5) * uint4(0x01000193u, 0x9E3779B1u, 0x85EBCA77u, 0xC2B2AE3Du);
        h = h.yzwx ^ (h >> 15);
        h = (h ^ v6) * uint4(0x01000193u, 0x9E3779B1u, 0x85EBCA77u, 0xC2B2AE3Du);
        h = h.yzwx ^ (h >> 15);
        h = (h ^ v7) * uint4(0x01000193u, 0x9E3779B1u, 0x85EBCA77u, 0xC2B2AE3Du);
        h = h.yzwx ^ (h >> 15);
    }
    [loop]
    for (; at < hi; at += 1024u) {
        h = (h ^ now.Load4(at)) * uint4(0x01000193u, 0x9E3779B1u, 0x85EBCA77u, 0xC2B2AE3Du);
        h = h.yzwx ^ (h >> 15);
    }
    parts[thread] = h;
    GroupMemoryBarrierWithGroupSync();
    if (thread != 0u) return;
    h = parts[0];
    [loop]
    for (uint i = 1u; i < 64u; ++i) {
        h = (h ^ parts[i]) * uint4(0x01000193u, 0x9E3779B1u, 0x85EBCA77u, 0xC2B2AE3Du);
        h = h.yzwx ^ (h >> 15);
    }
    if (compare != 0u) {
        // The flag, and above it 31 bits of the hash (`torn`).
        uint folded = h.x ^ (h.y * 3u) ^ (h.z * 5u) ^ (h.w * 7u);
        differs.Store(out_at + id.x * 4u, (folded << 1u) | (any(h != hashes.Load4(piece * 16u)) ? 1u : 0u));
        // With the copy-back on its own thread: the next compare of the
        // piece finds what is written from here on.
        if (compare == 2u) hashes.Store4(piece * 16u, h);
    } else {
        hashes.Store4(piece * 16u, h);
    }
}
