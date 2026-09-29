# MCU DDS SPDP Discovery — HIL Verification

How to reproduce the hardware-in-the-loop verification that the RP2350
(`meteor-pico`) device joins the DDS **SPDP** discovery multicast group and is
**discovered by a real DDS stack (Eclipse CycloneDDS)** — step 1 of the MCU
DDS/RTPS path (issue #328).

The MCU firmware (`crates/bin/mcu-rp2350/ns/src/spdp/`) joins `ff02::1234` and emits
a spec-correct RTPS **SPDP participant announcement** over UDP/IPv6 every ~3 s.
This is *TX-only discovery* — no user topics / DATA yet (that is the follow-on
"TMS conformance" step; see the end of this doc).

## Parameters (project ground truth)

| Item | Value |
|---|---|
| DDS domain | `0` |
| SPDP metatraffic multicast | `ff02::1234` port `7400` |
| Metatraffic unicast port | `7410` |
| Default unicast port | `7411` |
| Encoding | PL_CDR_LE (`00 03 00 00`) |
| Vendor id | `0xFF01` (custom / experimental — avoids OMG-allocated ids) |
| GuidPrefix | `vendorId(2) ++ MAC(6) ++ 00 00 00 01` = `ff01020000aabbcc00000001` |
| Participant GUID | guidPrefix ++ `00 00 01 c1` (`ENTITYID_PARTICIPANT`) |

The advertised unicast locator (`fe80::ff:feaa:bbcc`) is the device's real SLAAC
link-local address, derived from the MAC via the same EUI-64 math as
`lib_net::embassy::link_local_v6`.

## Topology — why a same-segment host is required

`ff02::` is IPv6 **link-local** multicast scope: it never crosses a router. The
device sits on its own L2 segment (e.g. `10.200.0.0/24`); a host on a *different*
subnet (e.g. the `10.0.0.0/24` bench) **cannot** receive the announce even though
it can route to the device. The verifier must sit on the **device's L2 segment**.

In this setup that host is **`hermes` (`10.200.0.140`, iface `ens18`)**, on the
same `/24` as the device (`10.200.0.195`). Any Linux host on that segment with
`python3` + `docker` works.

## Prerequisites

- Device flashed with the SPDP firmware and announcing:
  ```sh
  just mcu-flash-device        # builds ns --features defmt, flashes over SWD
  ```
  Confirm over RTT (`just mcu-rtt`): `spdp: joined ff02::1234` +
  `spdp: announce sent 212B` every ~3 s.
- A verifier host on the device's L2 segment with `python3` and `docker`.

## Method 1 — raw multicast receive (transport proof)

Confirms the announce leaves the device and reaches the segment. Save as
`spdp_recv.py` on the verifier host and run:

```python
import socket, struct, subprocess, time
GROUP, PORT = "ff02::1234", 7400
# pick the interface that carries the device subnet (edit the awk match if needed)
ifname = subprocess.check_output(
    "ip -o -4 addr show | awk '/10.200.0/{print $2; exit}'", shell=True).decode().strip()
idx = socket.if_nametoindex(ifname)
s = socket.socket(socket.AF_INET6, socket.SOCK_DGRAM)
s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(("", PORT))
s.setsockopt(socket.IPPROTO_IPV6, socket.IPV6_JOIN_GROUP,
             socket.inet_pton(socket.AF_INET6, GROUP) + struct.pack("@I", idx))
s.settimeout(16)
print(f"joined {GROUP}%{ifname} :{PORT}")
n, t = 0, time.time()
while time.time() - t < 16:
    try:
        data, addr = s.recvfrom(2048); n += 1
        print(f"#{n} from={addr[0]} len={len(data)} "
              f"magic={'RTPS' if data[:4]==b'RTPS' else '??'} "
              f"guidPrefix={data[8:20].hex()}")
    except socket.timeout:
        break
print(f"--- {n} SPDP packets ---")
```

Expected: several packets, `from=fe80::ff:feaa:bbcc`, `len=212`, `magic=RTPS`,
`guidPrefix=ff01020000aabbcc00000001`.

## Method 2 — full PL_CDR decode (semantic proof)

Parses every RTPS submessage + PL_CDR parameter and validates the values. Save as
`spdp_decode.py` (joins + receives one packet, then decodes):

```python
import socket, struct, subprocess
GROUP, PORT = "ff02::1234", 7400
ifname = subprocess.check_output(
    "ip -o -4 addr show | awk '/10.200.0/{print $2; exit}'", shell=True).decode().strip()
idx = socket.if_nametoindex(ifname)
s = socket.socket(socket.AF_INET6, socket.SOCK_DGRAM)
s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1); s.bind(("", PORT))
s.setsockopt(socket.IPPROTO_IPV6, socket.IPV6_JOIN_GROUP,
             socket.inet_pton(socket.AF_INET6, GROUP) + struct.pack("@I", idx))
s.settimeout(12)
data, addr = s.recvfrom(2048)
assert data[:4] == b"RTPS"
print("proto %d.%d vendor 0x%s guidPrefix %s" %
      (data[4], data[5], data[6:8].hex(), data[8:20].hex()))
PIDN = {0x15:"PROTOCOL_VERSION",0x16:"VENDORID",0x50:"PARTICIPANT_GUID",0x0f:"DOMAIN_ID",
        0x33:"METATRAFFIC_MCAST_LOC",0x32:"METATRAFFIC_UCAST_LOC",0x31:"DEFAULT_UCAST_LOC",
        0x58:"BUILTIN_ENDPOINT_SET",0x02:"LEASE_DURATION",0x01:"SENTINEL"}
def loc(v):
    return "kind=%d port=%d addr=%s" % (struct.unpack("<i", v[:4])[0],
           struct.unpack("<I", v[4:8])[0], socket.inet_ntop(socket.AF_INET6, v[8:24]))
i = 20
while i < len(data):
    sid, slen = data[i], struct.unpack("<H", data[i+2:i+4])[0]; body = data[i+4:i+4+slen]
    if sid == 0x15:
        print("DATA writer=%s (SPDP participant writer) encaps=%s" %
              (body[8:12].hex(), body[20:22].hex()))
        p = 24
        while p < len(body):
            pid = struct.unpack("<H", body[p:p+2])[0]; pl = struct.unpack("<H", body[p+2:p+4])[0]
            val = body[p+4:p+4+pl]; nm = PIDN.get(pid, hex(pid))
            if pid == 0x01: print("  SENTINEL"); break
            if pid in (0x33,0x32,0x31): print("  %s: %s" % (nm, loc(val)))
            elif pid == 0x0f: print("  %s: %d" % (nm, struct.unpack("<I", val[:4])[0]))
            elif pid == 0x02: print("  %s: %ds" % (nm, struct.unpack("<i", val[:4])[0]))
            else: print("  %s: %s" % (nm, val.hex()))
            p += 4 + pl
    i += 4 + slen
```

Expected: `DOMAIN_ID 0`, `METATRAFFIC_MCAST_LOC kind=2 port=7400 ff02::1234`,
unicast locators on ports 7410/7411 at `fe80::ff:feaa:bbcc`,
`BUILTIN_ENDPOINT_SET 0x3`, `LEASE_DURATION 100s`, clean parse to `SENTINEL`.

## Method 3 — CycloneDDS discovery (real DDS-stack interop)

Proves a mainstream DDS stack parses the SPDP and registers the participant.
No prebuilt CycloneDDS image exists, so build one from source (~2 min).

**1. Build the image** (`cyclone.Dockerfile`):

```dockerfile
FROM ubuntu:24.04
RUN apt-get update && apt-get install -y --no-install-recommends \
    git cmake g++ make ca-certificates && rm -rf /var/lib/apt/lists/*
RUN git clone --depth 1 https://github.com/eclipse-cyclonedds/cyclonedds /src && \
    cmake -S /src -B /build -DCMAKE_INSTALL_PREFIX=/usr/local \
      -DBUILD_IDLC=OFF -DENABLE_SECURITY=OFF -DENABLE_SSL=OFF && \
    cmake --build /build --target install -j"$(nproc)" && ldconfig
ENV LD_LIBRARY_PATH=/usr/local/lib
```
```sh
docker build -t cyclonedds -f cyclone.Dockerfile .
```
> For the follow-on TMS-conformance work, rebuild with `-DBUILD_IDLC=ON` plus
> `bison flex` in the apt line, to compile the TMS IDL (`docs/compliance/tms/`).

**2. CycloneDDS config** (`cyclonedds.xml`) — point it at our SPDP group + iface:

```xml
<CycloneDDS>
 <Domain Id="0">
  <General>
   <Interfaces><NetworkInterface name="ens18"/></Interfaces>
   <Transport>udp6</Transport>
   <AllowMulticast>spdp</AllowMulticast>
  </General>
  <Discovery>
   <SPDPMulticastAddress>ff02::1234</SPDPMulticastAddress>
  </Discovery>
 </Domain>
</CycloneDDS>
```
> Add `<Tracing><Category>discovery</Category></Tracing>` only to debug — it
> floods stderr and hides program output. Leave it out for a clean result.

**3. Discovery lister** (`disc.c`) — reads the builtin `DCPSParticipant` topic and
prints every discovered participant GUID, flagging the MCU:

```c
#include "dds/dds.h"
#include <stdio.h>
int main(void) {
    dds_entity_t pp = dds_create_participant(DDS_DOMAIN_DEFAULT, NULL, NULL);
    dds_entity_t rd = dds_create_reader(pp, DDS_BUILTIN_TOPIC_DCPSPARTICIPANT, NULL, NULL);
    void *s[20] = {0}; dds_sample_info_t si[20]; int found = 0;
    for (int t = 0; t < 25; t++) {
        dds_return_t n = dds_take(rd, s, si, 20, 20);
        for (dds_return_t i = 0; i < n; i++) {
            if (!si[i].valid_data) continue;
            unsigned char *k = (unsigned char *)&((dds_builtintopic_participant_t *)s[i])->key;
            printf("discovered participant guid: ");
            for (int b = 0; b < 16; b++) printf("%02x", k[b]);
            if (k[0]==0xff && k[1]==0x01 && k[2]==0x02 && k[5]==0xaa && k[6]==0xbb && k[7]==0xcc)
                { printf("   <== MCU (meteor-pico)"); found = 1; }
            printf("\n"); fflush(stdout);
        }
        if (n > 0) dds_return_loan(rd, s, n);
        dds_sleepfor(DDS_SECS(1));
    }
    dds_delete(pp);
    printf(found ? "RESULT: MCU PARTICIPANT DISCOVERED\n" : "RESULT: MCU not discovered\n");
    return found ? 0 : 2;
}
```

**4. Compile + run** (in the container, host networking for multicast on `ens18`):

```sh
docker run --rm --network host \
  -v "$PWD/disc.c:/disc.c" -v "$PWD/cyclonedds.xml:/cyclonedds.xml" \
  -e CYCLONEDDS_URI=file:///cyclonedds.xml -e LD_LIBRARY_PATH=/usr/local/lib \
  cyclonedds sh -c "gcc /disc.c -I/usr/local/include -L/usr/local/lib -lddsc -o /disc && /disc"
```

Expected:
```
discovered participant guid: ff01020000aabbcc00000001000001c1   <== MCU (meteor-pico)
RESULT: MCU PARTICIPANT DISCOVERED
```

## Reference SPDP packet (212 B)

```
52 54 50 53 02 04 ff 01  ff 01 02 00 00 aa bb cc  00 00 00 01 09 01 08 00   RTPS 2.4 + guidPrefix + INFO_TS
00 00 00 00 00 00 00 00  15 05 b0 00 00 00 10 00  ...                       DATA(SPDP writer) + PL_CDR params
```
Full hex is emitted over RTT on the first announce (`spdp: packet[212] = ...`).
Field-by-field layout + per-field dust_dds citations are in
`crates/bin/mcu-rp2350/ns/src/spdp/mod.rs`.

## Next step — TMS conformance over DDS

SPDP proves *discovery*. The MCU also publishes a TMS `DcMeasurementUpdate`
sample as RTPS **DATA** (XCDR2), announced via **SEDP**, so a CycloneDDS
`DataReader` built from the **idlc-compiled TMS IDL** matches, receives, and
decodes it — MIL-STD-3071 conformance through the one IDL. Verified below.

## TMS DATA conformance (CycloneDDS reader receives + decodes the sample)

The MCU encodes the sample with `lib_mapping::render_cdr` (generated from the
same IDL), publishes DATA to `ff02::1234:7401`, and announces the writer via
SEDP `DiscoveredWriterData` (+ an RTPS HEARTBEAT, since builtin SEDP is
reliable) to `ff02::1234:7400`. Topic `"DcMeasurementUpdate"`, type
`"tms::dc::MeasurementUpdate"` — both derived from the IDL, so they match idlc.

**1. Build CycloneDDS *with idlc*** (extend the Method-3 Dockerfile): add
`bison flex` to the apt line and `-DBUILD_IDLC=ON -DENABLE_TYPELIB=ON` to the
cmake flags; tag `cyclonedds-idlc`.

**2. Compile the TMS IDL → C types:**
```sh
cp docs/compliance/tms/mil-std-3071_data_model.idl /tmp/tms_model.idl
docker run --rm -v /tmp/tms_model.idl:/tms_model.idl -w /out -v /tmp/idlout:/out \
  cyclonedds-idlc idlc /tms_model.idl          # -> /tmp/idlout/tms_model.{c,h}
```
idlc emits `tms_dc_MeasurementUpdate` + `tms_dc_MeasurementUpdate_desc`
(`.m_typename = "tms::dc::MeasurementUpdate"`), matching the MCU's SEDP.

**3. Subscriber** (`sub.c`) — BEST_EFFORT reader (matches the writer's test QoS):
```c
#include "dds/dds.h"
#include "tms_model.h"
#include <stdio.h>
int main(void) {
    dds_entity_t pp = dds_create_participant(0, NULL, NULL);
    dds_entity_t tp = dds_create_topic(pp, &tms_dc_MeasurementUpdate_desc,
                                       "DcMeasurementUpdate", NULL, NULL);
    dds_qos_t *q = dds_create_qos();
    dds_qset_reliability(q, DDS_RELIABILITY_BEST_EFFORT, 0);
    dds_entity_t rd = dds_create_reader(pp, tp, q, NULL); dds_delete_qos(q);
    void *s[1] = {NULL}; dds_sample_info_t si[1];
    for (int t = 0; t < 40; t++) {
        if (dds_take(rd, s, si, 1, 1) > 0 && si[0].valid_data) {
            tms_dc_MeasurementUpdate *m = s[0];
            printf("deviceId='%s' external._length=%u\n",
                   m->deviceId, m->externalMeasurement._length);
            return 0;
        }
        dds_sleepfor(DDS_SECS(1));
    }
    return 2;
}
```

**4. Config** — set `<AllowMulticast>true</AllowMulticast>` (not just `spdp`)
so the reader joins the writer's `ff02::1234:7401` data group. Run:
```sh
docker run --rm --network host -v $PWD/sub.c:/sub.c -v /tmp/idlout:/idlout \
  -v $PWD/cyclonedds.xml:/c.xml -e CYCLONEDDS_URI=file:///c.xml \
  -e LD_LIBRARY_PATH=/usr/local/lib cyclonedds-idlc \
  sh -c "gcc /sub.c /idlout/tms_model.c -I/idlout -I/usr/local/include -lddsc -o /sub && /sub"
```
Expected:
```
WRITER MATCHED (via SEDP): total_count=1
deviceId='METEOR-001' external._length=1
```

**Debugging discovery** — enable `<Tracing><Category>discovery</Category></Tracing>`
and grep for the MCU participant `ff010200:aabbcc:1:1c1`: `bes 7` confirms the
SEDP PUBLICATIONS_ANNOUNCER bit; a `proxy_writer ... out-of-sync` that never
clears means the reliable SEDP reader is missing the HEARTBEAT.
