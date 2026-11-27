
#define RS "RootConstants(num32BitConstants=5, b0), SRV(t0), SRV(t1), UAV(u0)"
cbuffer Look : register(b0) { uint at; uint words; uint was; uint now; uint found_at; };
ByteAddressBuffer card : register(t0);
ByteAddressBuffer cpu : register(t1);
RWByteAddressBuffer found : register(u0);
[RootSignature(RS)]
[numthreads(64, 1, 1)]
void main(uint3 id : SV_DispatchThreadID) {
    if (id.x >= words) return;
    uint k = id.x * 4u, n;
    uint c = card.Load(at + k), w = cpu.Load(was + k);
    if (c == w || cpu.Load(now + k) != w) return;
    found.InterlockedAdd(found_at, 1u, n);
    if (n == 0u) found.Store3(found_at + 4u, uint3(at + k, c, w));
}
