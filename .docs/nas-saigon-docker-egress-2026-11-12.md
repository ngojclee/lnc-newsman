# Chẩn đoán egress Docker trên NAS Saigon (ASUSTOR) — 2026-11-12

**Phạm vi**: READ-ONLY, không thay đổi gì trên NAS. SSH `immich-host` (10.20.2.1:2205, root).
**Kết luận**: đã chốt root cause, kèm phương án sửa chờ owner duyệt.

## TL;DR

Bảng nftables `ip Asustor_Firewall_V4` của ADM Defender đang là **snapshot đông cứng từ 07/10 lúc 04:54** — lần "Apply Firewall Rules" cuối cùng trong log của `defenderd`. Docker chạy với `--iptables=false` (ADM cố ý trong `start-stop.sh`), nên **dockerd không tự ghi rule**; mọi rule masquerade/forward cho bridge do **defenderd** render, và nó chỉ render lại khi dockerd khởi động. Mọi bridge tạo sau 07/10 (`172.24`, `172.27`, `172.29`, `172.30`, `172.31`, `192.168.0.0/20`, `192.168.16.0/20`) không có rule `masquerade` → gói tin ra ngoài mang source private → reply không bao giờ quay lại → mọi đích ngoài host đều timeout.

## Verified (đo trực tiếp)

| # | Bằng chứng | Giá trị |
|---|---|---|
| 1 | NAT masquerade tồn tại **chỉ** cho 10 subnet cũ | `Postrouting` chain trong `Asustor_Firewall_V4`: 172.17, .18, .19, .20, .21, .22, .23, .25, .26, .28 — **không có** 172.24/27/29/30/31 và 192.168.x/20 |
| 2 | Packet đi ra nhưng không được NAT | conntrack: `SYN_SENT src=172.30.0.2 dst=1.1.1.1 [UNREPLIED] src=1.1.1.1 dst=172.30.0.2` (reply tuple không đổi source); so với bridge cũ: `ESTABLISHED src=172.20.0.4 … dst=10.20.2.1` (đã masq về IP LAN của NAS) |
| 3 | dockerd không ghi rule | cmdline: `dockerd --debug --data-root … --iptables=false`; `iptables -t nat/filter/mangle/raw -S` (legacy) rỗng hoàn toàn; `nft list table ip nat` chỉ có chain rỗng |
| 4 | Lần apply cuối của defenderd | `/usr/builtin/etc/defender/defenderutil.log`: `2026-10-07 04:54:10 [INFO] Apply Firewall Rules` — **không có apply nào sau đó**; trùng đúng mốc dockerd start (log `Starting up` 04:54:00) |
| 5 | Ai trigger apply | `start-stop.sh`: `defenderutil -s start_docker_daemon && sleep 1 && defenderutil -a`, và chỉ gọi khi daemon start; không cron, không netlink watcher khác |
| 6 | Tập bridge trong rule == tập network restore lúc 04:54 | rules tham chiếu `br-ed158047e5d6`, `br-0ab21ea82bae` — bridge **đã chết** (thấy "restored" trong dockerd.log), tức render từ snapshot thời điểm đó; `ebooks_default`/`docker-autoprune` tạo lại sau vẫn "sống nhờ" rule cũ theo đúng subnet |
| 7 | Rule writer = ASUSTOR | `defenderd` (PID 4917, chạy từ boot) + `defenderutil` (27KB, có `Nftables_Config_Writer`, `DOCKER-USER`, `Apply_Nftables_Profile`); docker-ce package `29.6.2.r52` maintainer=ASUSTOR |
| 8 | Scope ảnh hưởng rộng hơn 3 stack | cũng thiếu rule cho `lnc-phone` (172.24, tạo 09/11), `lnc-social-dl` (172.27, 10/11 — host-mode nên không sao), `lnc-obsidian` + `lnc-obsidian-agent` (192.168.0/20 + 192.168.16/20, 12/11) |
| 9 | `LOCALIPS` set tươi hơn table | chứa tới `172.28.0.1` + `172.24.0.1`, `172.27.0.1` → ADM có cơ chế update lan-section riêng, chạy được tới ~10–11/11 rồi cũng dừng |
| 10 | Lỗi phụ | `/dev/stdin|stdout|stderr|fd` không tồn tại → dockerd gọi `nft` lỗi `Could not open file "/dev/stdin"` (1 lần, IPv6 delete lúc start — cosmetic, không phải nguyên nhân chính) |

## Cơ chế lỗi (tóm tắt luồng)

1. `start-stop.sh` (cron `@reboot` + App Central) start `dockerd … --iptables=false` khi `defenderutil --docker_support` = true.
2. Ngay sau start, script gọi `defenderutil -s start_docker_daemon` + `defenderutil -a` → **defenderd render toàn bộ `Asustor_Firewall_V4`**: `Postrouting` masquerade theo subnet, `DOCKER*` chains (CT, ISOLATION, BRIDGE, FORWARD-accept, NAT publish-port), `LOCALIPS`, `TrustedIP`, Defender block rules.
3. Sau đó **không có trigger nào** cho `docker network create` mới → table đứng yên tới lần dockerd start kế tiếp.
4. Bridge cũ sống được vì subnet của chúng nằm trong snapshot (kể cả khi bridge được recreate, subnet lặp lại đúng rule cũ — như 172.18/172.23). Bridge mới với subnet chưa từng thấy → không masquerade → egress chết.
5. Cùng host vẫn thông vì NAS route trực tiếp giữa các bridge (kernel route `172.x dev br-…`), không cần NAT; "gateway 172.31.0.1 refused" = RST từ host (vẫn tới được, chỉ là không ai listen).

## Đã loại trừ

- MTU (mọi bridge 1500), `ip_forward=1`, forwarding per-bridge, link/carrier, DNS — session trước.
- **Không phải** forward bị drop: `ip filter` FORWARD policy accept + `DOCKER_FORWARD` không có drop cho iifname mới; conntrack cho thấy SYN đi qua forward path.
- **Không phải** OPNsense/static-route thiếu: packet không được NAT nên kể cả route trả về cũng vô nghĩa; và LAN cũng fail dù cùng L2 domain (reply nhầm next-hop vì source private).
- Không phải docker network options: mọi bridge cùng driver `bridge`, không `internal`, không `enable_ip_masquerade=false` (docker đâu có ghi được gì — `--iptables=false`).

## Phương án sửa (xếp theo độ an toàn) — **Pending owner approval**

### A. Bổ sung rule thủ công vào `Asustor_Firewall_V4` — restore nhanh, không restart gì

```sh
# cho từng subnet thiếu (ví dụ newsman):
nft add rule ip Asustor_Firewall_V4 Postrouting \
  ip saddr 172.31.0.0/16 oifname != "br-56df4a44cd2b" masquerade
nft add rule ip Asustor_Firewall_V4 DOCKER_FORWARD \
  iifname "br-56df4a44cd2b" accept
```

Lặp cho: `172.29.0.0/16 br-67ab6b37587f`, `172.30.0.0/16 br-cd9fea485fe7`, `172.31.0.0/16 br-56df4a44cd2b` (+ `172.24`, `172.27`, `192.168.0.0/20`, `192.168.16.0/20` nếu các stack đó cần egress).
**Ưu**: hiệu lực tức thì, blast radius ~0, rollback = `nft delete rule` theo handle.
**Khuyết**: mất khi reboot (nft không persist) hoặc khi ADM apply lại; chỉ là bản vá tạm.

### B. `defenderutil -s start_docker_daemon && defenderutil -a` — đúng cơ chế của ADM (đề xuất chính)

Replicate y hệt đoạn start-stop.sh chạy sau dockerd start → defenderd re-render toàn bộ profile với **tập bridge hiện tại** → tất cả subnet mới có masquerade + DOCKER chains.
**Ưu**: đúng đường của hệ thống, fix luôn LOCALIPS/stale chains, không cần restart container nào.
**Khuyết**: apply cả profile ADM Defender (Input/Filter/AbuseIP) — có thể gián đoạn rất ngắn và áp lại policy Defender; nên làm trong cửa sổ bảo trì. Chưa chắc `-s` kích hoạt re-render docker section nếu không kèm `-a` — dùng đủ cặp như script gốc.
**Verify sau**: `nft list chain ip Asustor_Firewall_V4 Postrouting` phải thấy 172.29/30/31; `docker exec lnc-newsman python3 -c "socket.connect(('1.1.1.1',443))"` phải OK.

### C. Restart docker-ce qua App Central — thay thế cho B nếu B không re-render được

`start-stop.sh stop/start` → đúng flow boot → apply mới nhất. **Khuyết**: stop/start toàn bộ container trên NAS (Portainer, immich, vaultwarden…) — blast radius lớn nhất, chỉ chọn khi A+B không đủ.

### D. Workaround per-stack: `network_mode: host`

Như lnc-social-dl/wolweb — vượt qua hoàn toàn. Chỉ nên cho từng stack cần gấp; đổi compose + redeploy qua Portainer (mất port-mapping semantics, đổi cách expose).

### Dài hạn (report upstream + canh)

- Lỗi thiết kế của ASUSTOR: **không có watcher cho `docker network create`** — mọi network tạo giữa hai lần dockerd start đều không có firewall rule (trước giờ được "che" nhờ subnet reuse/stale rules). Báo ASUSTOR kèm bằng chứng này.
- `/dev/std*` thiếu khiến call `nft` của dockerd lỗi — tạo symlink trong init/boot script (ADM tự populate /dev, cần persist qua script boot) hoặc báo ASUSTOR.
- Cân nhắc watchdog: so `docker network ls` vs `Postrouting` rules, lệch → cảnh báo/chạy lại `-a` (sau khi B được duyệt là an toàn).

## Câu hỏi mở (không chặn fix)

- Ai/cái gì update `LOCALIPS` giữa 10–12/11 (set có 172.24/172.27 .1 nhưng không có 172.29–31) — lan-section có đường update riêng, đã dừng sau ~10/11.
- Sao dockerd restart 04:54 07/10 (không phải boot — NAS up từ ~03/10): có thể do ADM/docker-ce update — không điều tra thêm.

---

## Applied — fix đã chạy + verify (2026-11-12 ~22:40)

**Owner duyệt phương án B.** Đã chạy trên NAS:

```sh
defenderutil -s start_docker_daemon && defenderutil -a
```

- `-s` trả `Failed to connect to master socket` (×5) — **socket của defenderd không connect được** (daemon PID 4917 tồn tại nhưng socket chết → đây có lẽ là lý do không có auto-resync nào suốt 36 ngày). `-a` vẫn ghi rules thành công (rc=0).
- `Postrouting` trước = 10 rule (snapshot cũ); sau = **17 rule**, đủ mọi bridge hiện tại với đúng `br-*` đang sống (xóa sạch stale names): 172.17–172.31 đầy đủ + `192.168.0.0/20` + `192.168.16.0/20`.

**Verify sau fix (verified):**

| Test | Kết quả |
|---|---|
| `nft list chain ip Asustor_Firewall_V4 Postrouting` | masquerade cho 172.29/30/31, 172.24, 172.27, 192.168.0/20, 192.168.16/20 ✅ |
| TCP `1.1.1.1:443` từ lnc-cwa-trial (172.30) | **OPEN** |
| TCP `1.1.1.1:443` từ lnc-library (172.29) | **OPEN** |
| TCP `1.1.1.1:443` từ lnc-newsman (172.31) | **OPEN** |
| TCP `10.20.0.1:53` (gateway LAN) từ cả 3 | **OPEN** |
| conntrack reply tuple | `src=172.31.0.2 dst=1.1.1.1 … src=1.1.1.1 dst=10.20.2.1 [ASSURED]` — đã masq về IP LAN ✅ |

**Egress đã phục hồi trên cả 3 stack** (và 172.24, 172.27, 2×192.168.x nếu trước đó cũng chết).

## Applied 2 — restart defenderd + hành vi auto-sync đã định lượng (2026-11-12 ~22:50)

**Owner duyệt restart defenderd** để vá socket chết.

- **Cách restart**: KHÔNG dùng `S99defenderd restart|stop` — `do_stop` chạy `defenderutil -R` (**flush toàn bộ firewall**). Dùng `kill -TERM <pid>` rồi `S99defenderd start` — daemon mới lên không mất rule nào.
- Daemon init chậm: socket `defender_socket` chỉ xuất hiện sau **~30–60s** (các test `-s` ngay sau start cho "Failed to connect" giả do race — phải chờ).
- **Verify sau restart (verified)**: PID mới 23146, `/usr/builtin/etc/defender/defender_socket` bound+listening, `defenderutil -s start_docker_daemon` **im lặng rc=0** (không còn lỗi socket), `LOCALIPS` render lại đủ 172.17–172.31 + 192.168.x.

**Probe test (tạo `lnc-egress-probe` 192.168.32.0/20 rồi xoá):**

| Hành vi | Kết quả |
|---|---|
| `docker network create` → rule tự sinh? | **KHÔNG** — chờ 8s+ không có rule. Daemon không watch docker events |
| `defenderutil -s start_docker_daemon` | rule `192.168.32.0/20 masquerade` xuất hiện **~6s** — đây mới là trigger |
| `docker network rm` → rule tự dọn? | **KHÔNG** — rule sót lại kể cả sau `-a` (apply là append-only, giống hệt stale rules lịch sử) |
| Dọn tay | `nft delete rule … handle N` cho masquerade + các chain DOCKER_* + `nft delete element … LOCALIPS` — trạng thái sạch: **17 rule**, 0 remnant |

**Quy trình vận hành đúng từ đây** (trong chừng nào chưa có fix từ ASUSTOR): **sau khi tạo stack/network mới, chạy `defenderutil -s start_docker_daemon`** (thêm rule cho subnet mới trong ~6s, không restart gì). Không có cơ chế auto nào — bản chất bug là daemon chỉ sync khi được gọi.

## Follow-up còn nợ

1. **`defenderd` socket chết** — ✅ ĐÃ GIẢI QUYẾT (restart, socket sống). Nhưng auto-sync per-create **không tồn tại theo thiết kế** → chỉ có message/apply mới sync → phải chạy `-s`/`-a` sau mỗi lần tạo network, hoặc đề xuất owner: cron/watchdog so `docker network ls` vs `Postrouting`, hoặc ticket ASUSTOR.
2. `/dev/std*` symlinks thiếu → dockerd gọi nft lỗi (cosmetic).
3. `defenderutil -s`/`stop`/`restart`/`stop_docker_daemon` message semantics — chưa map hết (`stop` flush firewall — tránh).
