
#define RS "SRV(t0), SRV(t1), SRV(t2), SRV(t3), SRV(t4), UAV(u0), UAV(u1)"
ByteAddressBuffer src0 : register(t0);
ByteAddressBuffer src1 : register(t1);
ByteAddressBuffer src2 : register(t2);
ByteAddressBuffer src3 : register(t3);
// The count, 12 bytes of nothing, then per job: source (bits 0-1), kind
// (4-7), words (8-11) and the executions that read it (16-31); the
// record's offset in the source; its place in the scratch buffer; nothing.
ByteAddressBuffer jobs : register(t4);
RWByteAddressBuffer dst_args : register(u0);
RWByteAddressBuffer budget : register(u1);
static const uint BUDGET = 1u << 27;
static const uint GROUPS = 1u << 20;
groupshared uint costs[64];
groupshared uint fits[64];
groupshared uint largest_asked;
groupshared uint all_asked;
uint load(uint source, uint at) {
    if (source == 0) return src0.Load(at);
    if (source == 1) return src1.Load(at);
    if (source == 2) return src2.Load(at);
    return src3.Load(at);
}
[RootSignature(RS)]
[numthreads(64, 1, 1)]
void main(uint3 id : SV_DispatchThreadID, uint gi : SV_GroupIndex) {
    uint count = jobs.Load(0);
    bool active = id.x < count;
    uint4 job = uint4(0, 0, 0, 0);
    uint record[5] = {0, 0, 0, 0, 0};
    uint cost = 0;
    uint asked = 0;
    uint kind = 0;
    uint words = 0;
    if (gi == 0) {
        largest_asked = 0;
        all_asked = 0;
    }
    // As the clamp of one execution (`super::SOURCE`) bounds them.
    if (active) {
        job = jobs.Load4(16 + id.x * 16);
        kind = (job.x >> 4) & 15;
        words = min((job.x >> 8) & 15, 5);
        for (uint w = 0; w < words; w++) {
            record[w] = load(job.x & 3, job.y + w * 4);
        }
        if (kind <= 1) {
            if (record[0] >= 1u << 23) {
                record[0] = 0;
                record[1] = 0;
            }
            record[1] = min(record[1], (1u << 26) / max(record[0], 1u));
            cost = record[0] * record[1];
        } else if (kind == 2) {
            uint3 g = uint3(record[0], record[1], record[2]);
            g = min(g, uint3(g.x <= WIDEST ? WIDEST : 65535u, 65535u, 65535u));
            if ((g.y != 0 && g.x > GROUPS / g.y) || (g.z != 0 && g.x * g.y > GROUPS / g.z)) {
                g = uint3(0, 0, 0);
            }
            cost = g.x * g.y * g.z * 64u;
            record[0] = g.x;
            record[1] = g.y;
            record[2] = g.z;
        } else {
            if (record[0] > 65535u) {
                record[0] = 0;
                record[1] = 0;
                record[2] = 0;
            }
            asked = record[0];
            record[1] = min(record[1], 1u);
            record[2] = min(record[2], 1u);
            cost = record[0] * record[1] * record[2] * 128u;
        }
        cost *= job.x >> 16;
    }
    costs[gi] = cost;
    GroupMemoryBarrierWithGroupSync();
    if (active && kind > 2) {
        uint was;
        InterlockedMax(largest_asked, asked, was);
        InterlockedAdd(all_asked, min(asked, 1u << 24), was);
    }
    uint first = id.x - gi;
    uint n = count > first ? min(64u, count - first) : 0u;
    if (gi == 0 && n > 0) {
        uint before;
        budget.InterlockedOr(0, 0, before);
        [allow_uav_condition]
        while (before < BUDGET) {
            uint next = before;
            for (uint j = 0; j < n; j++) {
                uint c = costs[j];
                if (c > 0 && next < BUDGET) {
                    next = (BUDGET - next < c) ? (REST ? BUDGET : next) : (next + c);
                }
            }
            if (next == before) break;
            uint prev;
            budget.InterlockedCompareExchange(0, before, next, prev);
            if (prev == before) break;
            before = prev;
        }
        uint at = before;
        uint refused = 0;
        uint biggest = 0;
        for (uint k = 0; k < n; k++) {
            uint c = costs[k];
            uint fit = 1;
            if (c > 0) {
                fit = (at < BUDGET && BUDGET - at >= c) ? 1u : 0u;
                at = (at >= BUDGET || BUDGET - at < c) ? (REST ? BUDGET : at) : (at + c);
                refused += 1 - fit;
                biggest = max(biggest, c);
            }
            fits[k] = fit;
        }
        // Diagnostics: the records that run empty for want of budget, and
        // the largest cost there was (small ones are not told).
        uint told;
        if (refused > 0) {
            budget.InterlockedAdd(12, refused, told);
        }
        if (biggest >= (1u << 22)) {
            budget.InterlockedMax(16, biggest, told);
        }
    }
    GroupMemoryBarrierWithGroupSync();
    if (gi == 0 && n > 0 && all_asked > 0) {
        uint was;
        budget.InterlockedMax(4, largest_asked, was);
        budget.InterlockedAdd(8, all_asked, was);
    }
    if (!active) {
        return;
    }
    if (fits[gi] == 0) {
        record[0] = 0;
        record[1] = 0;
        if (kind > 1) {
            record[2] = 0;
        }
    }
    for (uint s = 0; s < words; s++) {
        dst_args.Store(job.z + s * 4, record[s]);
    }
}
