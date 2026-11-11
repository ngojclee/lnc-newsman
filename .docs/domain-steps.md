# Domain cho Newsman — **ĐÃ ÁP DỤNG** (2026-11-11)

Trạng thái: **đã làm bởi phiên khác trong ngày 2026-11-11**, không phải phiên này.
`newsman.lengoc.me` → `10.20.0.3` (edge Caddy VN) → `10.20.2.1:8462`, HTTPS trả 303 về
`/login.html`, `ssl_verify_result=0`. Bản ghi DNS và site block Caddy **không do phiên này
sửa**; phiên này chỉ đổi env của stack và sửa lỗi giới hạn đăng nhập, rồi kiểm lại.

Những gì đã kiểm trong phiên này (qua `https://newsman.lengoc.me`):
`/health` 200 · `/login.html` 200 · đăng nhập bằng `MANAGEMENT_KEY` 200, cookie `Secure; HttpOnly`
· `/api/v1/regions` 200 · `/api/admin/regions` trả đủ 5 mã (`global`, `eu`, `fr`, `us`, `vn`),
chỉ `global` bật, effective `['global']`, pack issues `[]`.

Các mục 2–4 dưới đây giữ lại làm bản ghi cách làm gốc và cách hoàn tác. Mục 1 (câu hỏi cần
chủ dự án xác nhận) coi như đã được quyết: phạm vi LAN + `netbird`, HTTPS, không bản ghi công khai.

## 1. Việc cần chủ dự án xác nhận trước (skill yêu cầu hỏi, không tự đoán)

1. **Ai được phân giải tên** `newsman.lengoc.me`? Mặc định LAN + `admin-remote` (NetBird).
   Nếu muốn mở cho nhóm mesh rộng hơn, phải sửa thêm `NB_DNS_ADMIN` trên **cả hai** OPNsense
   trong cùng thay đổi.
2. **Ai được mở** trang? Mặc định mọi VLAN không phải Guest + `netbird`. Muốn giới hạn thì đặt
   `allow:` và phải thêm ACL tương ứng trong cùng lần sửa Caddyfile.
3. **Có thay đổi bề mặt công khai không?** Việc này chỉ thêm bản ghi nội bộ; **không** tạo
   Cloudflare record. Nếu sau này muốn công khai, đó là thay đổi riêng cần duyệt riêng.
4. **HTTPS hay HTTP?** Caddy sẽ lo TLS bằng DNS-01; khi đó đặt `COOKIE_SECURE=true` trong stack
   và redeploy (đổi env phải qua git redeploy, không sửa compose trong Portainer).

## 2. Thay đổi đề xuất (theo đúng thứ tự của skill: edge trước, DNS sau)

### 2.1 Site block Caddy (site VN, nơi app chạy)

File trên CT104 VN: `/srv/caddy-fleet/Caddyfile.vn`. Sửa **tại chỗ** để giữ inode của
bind-mount (dùng `cat > file` hoặc `scp` đè nội dung; **không** `sed -i`, vì nó đổi inode và
container tiếp tục phục vụ file cũ).

```caddy
# newsman.lengoc.me — LNC Newsman (internal, LAN + NetBird)
# App: stack lnc-newsman (Portainer nas-vn, endpoint 2), 10.20.2.1:8462
newsman.lengoc.me {
	encode zstd gzip
	reverse_proxy 10.20.2.1:8462 {
		header_up X-Forwarded-For {remote_host}
		header_up X-Real-IP {remote_host}
	}
	# Nếu giới hạn VLAN, thêm ACL ngay trong cùng lần sửa:
	# @acl_newsman not remote_ip <CIDR từ dnsctl.py acl>
	# handle @acl_newsman { respond 403 }
	header {
		Strict-Transport-Security "max-age=31536000; includeSubDomains"
		X-Content-Type-Options nosniff
		-X-Powered-By
	}
	log {
		output file /var/log/caddy/newsman.log
	}
}
```

Sau khi sửa: `caddy validate --config /srv/caddy-fleet/Caddyfile.vn` → `caddy reload`.
Kiểm **trước khi đụng DNS**:

```bash
curl -s -o /dev/null -w '%{http_code} %{ssl_verify_result}\n' \
  --resolve newsman.lengoc.me:443:10.20.0.3 https://newsman.lengoc.me/
# kỳ vọng: 200/302 và 0
```

Lưu ý chứng chỉ: DNS-01 cần thời gian; nếu trả lỗi TLS ngay sau reload, chờ ~15 giây rồi thử lại.

Đồng bộ ngược về canonical repo `automations/edge-fleet` (`caddy/deploy/Caddyfile.vn`) và chạy
`tools/edge_drift.sh` phải exit 0 (change-gates mục 7).

### 2.2 Bản ghi DNS nội bộ

Repo `automations/internal-dns` → `records.yaml`, thêm:

```yaml
newsman:
  name: newsman.lengoc.me
  target: 10.20.0.3        # edge Caddy VN
  backend: 10.20.2.1:8462  # stack lnc-newsman
  note: LNC Newsman internal reader (LAN only, no public record)
  # allow: [vn-lan, vn-server, vn-work, vn-home, netbird]   # chỉ khi duyệt giới hạn
```

Sau đó:

```bash
pct exec 108 -- python3 /opt/internal-dns/dnsctl.py plan
pct exec 108 -- python3 /opt/internal-dns/dnsctl.py apply --env /etc/internal-dns/dnsctl.env
pct exec 109 -- python3 /opt/internal-dns/dnsctl.py plan    # FR dùng chung records
pct exec 109 -- python3 /opt/internal-dns/dnsctl.py apply --env /etc/internal-dns/dnsctl.env
```

Hoặc chờ timer tự áp dụng sau khi change set ổn định 10 phút. Mỗi lần apply = một lần restart
Unbound (~11 giây DNS outage) trên site đó — gộp nhiều tên vào một commit nếu được.

### 2.3 Đổi env khi đã có HTTPS

Trong Portainer: stack `lnc-newsman` → Editor → Environment variables → `COOKIE_SECURE=true`
→ **Update the stack** (UI gửi method `git`, giữ liên kết repo). Hoặc API:

```
PUT /api/stacks/108/git/redeploy?endpointId=2
{"env":[...đầy đủ env hiện tại, COOKIE_SECURE=true...],"repositoryReferenceName":"refs/heads/main","pullImage":true}
```

**Không** `PUT /api/stacks/108` kèm `stackFileContent` — sẽ biến stack repo thành file stack và
xoá GitOps workflow.

Sau redeploy, đăng nhập lại qua `https://newsman.lengoc.me/login.html`.

## 3. Verify sau khi làm (đủ mới coi là xong)

1. `curl --resolve` vào edge → 200/302, `ssl_verify_result=0`.
2. `getent hosts newsman.lengoc.me` trên runner mỗi site → `10.20.0.3`.
3. Từ LAN VN và LAN FR: mở `https://newsman.lengoc.me/login.html`, đăng nhập bằng khóa trong
   `F:\_agentsync\.secrets\newsman\keys.txt`.
4. `/api/v1/regions` 200, cây vùng đủ 5.
5. Nếu có `allow:`: thử từ VLAN bị loại → **403**.
6. Truy cập từ ngoài (1.1.1.1) → NXDOMAIN (không có record công khai).

## 4. Hoàn tác (theo thứ tự ngược)

1. Portainer: đặt `COOKIE_SECURE=false` (nếu đã bật) và redeploy.
2. `records.yaml`: xoá mục `newsman`, commit, push; `dnsctl.py apply` cả hai site → override bị
   xoá tự động.
3. Caddyfile VN: xoá site block, `caddy validate` + `caddy reload`; đồng bộ lại
   `automations/edge-fleet`; `tools/edge_drift.sh` exit 0.
4. Stack vẫn chạy LAN-only tại `http://10.20.2.1:8462` như hiện tại — không cần xoá.

## 5. Trạng thái sau khi áp dụng (2026-11-11)

- Env stack 108 đã đổi qua `PUT /api/stacks/108/git/redeploy` (không dùng `stackFileContent`):
  `COOKIE_SECURE=true`, `ALLOWED_HOSTS=newsman.lengoc.me,10.20.2.1`, `TRUSTED_PROXIES=10.20.0.3`.
  GitOps vẫn là workflow 5, hash deploy `c81f9df0…`, source/artifact/target đều healthy.
- **Hệ quả cần biết:** cookie giờ là `Secure`, nên đăng nhập qua `http://10.20.2.1:8462` **không
  giữ phiên** trong trình duyệt. Địa chỉ LAN vẫn trả `/health` và API có key, nhưng đường đăng
  nhập dùng được là `https://newsman.lengoc.me/login.html`.
- **Giới hạn đăng nhập** đã sửa (commit `c81f9df`): chỉ tin `X-Forwarded-For` khi peer là
  `10.20.0.3`, và chỉ lấy mục **cuối** hợp lệ trong chuỗi; mục do client tự thêm bị bỏ qua.
- **`ALLOWED_HOSTS` đã bật**: Host lạ bị 400. Thêm tên miền mới phải sửa env rồi redeploy.

## 6. Rủi ro riêng của bước domain

- **Đăng nhập sau proxy**: giới hạn đăng nhập đang đếm theo địa chỉ TCP peer. Sau Caddy, mọi
  người dùng sẽ cùng một IP peer → một người thử sai có thể khoá cả nhóm trong 5 phút. Đây là
  hạn chế **đã biết, chưa sửa** (cần trusted-proxy/`X-Forwarded-For` có kiểm soát). Cân nhắc sửa
  trước khi mở cho nhiều người.
- **Giới hạn tên miền**: `ALLOWED_HOSTS` đang trống (chấp nhận mọi Host header). Sau khi có
  domain nên đặt `ALLOWED_HOSTS=newsman.lengoc.me,10.20.2.1` và redeploy.
- **TLS + cookie**: quên `COOKIE_SECURE=true` sau khi có HTTPS nghĩa là cookie vẫn đi qua HTTP
  nếu ai đó truy cập bằng `http://`; ngược lại, bật `true` khi vẫn dùng HTTP sẽ làm đăng nhập
  hỏng trên LAN.
