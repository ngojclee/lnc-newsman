# Bàn giao domain nội bộ — Newsman và Library (chưa áp dụng)

Trạng thái: **kế hoạch đã chốt, chưa tạo bản ghi DNS, chưa sửa edge, chưa đổi proxy.**
Mọi bước cần chủ dự án duyệt từng bước.

## 1. Bảng gán domain → IP/backend

| Domain | Dùng cho | Hôm nay | Sau khi áp dụng | Trạng thái |
|---|---|---|---|---|
| `newsman.lengoc.me` | LNC Newsman | chỉ truy cập `http://10.20.2.1:8462` | DNS nội bộ → `10.20.0.3` (edge Caddy VN, CT104) → `10.20.2.1:8462` | Chờ duyệt (`domain-steps.md`) |
| `calibre.lengoc.me` | CWA (trang quản lý kho, người ghi duy nhất) | chưa có | DNS nội bộ → edge → `10.20.2.1:8083` (CWA trong stack #88) | Bước 1, chờ duyệt (`cutover-ebooks.md`) |
| `ebooks.lengoc.me` | Library (đọc/nghe/dịch) | đang trỏ Calibre-Web `10.20.2.1:8083` | đổi upstream → `10.20.2.1:8463` (Library, repo-stack) | Bước 2, chỉ làm sau khi `calibre.lengoc.me` đã kiểm xong |
| — | Truy cập trực tiếp (dự phòng) | `10.20.2.1:8462` (Newsman), `10.20.2.1:8463` (Library) | không đổi | đang chạy |

Chi tiết theo từng app:

- **Newsman** (`lnc-newsman`, stack #108, endpoint 2, data `/share/Docker/lnc-newsman`): site block Caddy VN, `reverse_proxy 10.20.2.1:8462`. Bản ghi `records.yaml`: `newsman` → target `10.20.0.3`, backend `10.20.2.1:8462`. Sau HTTPS đặt `COOKIE_SECURE=true` qua git redeploy.
- **Library** (`lnc-library`, repo-stack #109 port 8463, image 0.3.5; #106 đang stop để hoàn tác): `ALLOWED_HOSTS` đã có `ebooks.lengoc.me,10.20.2.1,lnc-library`; `PUBLIC_BASE_URL=https://ebooks.lengoc.me`. Cookie `lnc_library_session` chỉ trên `ebooks.lengoc.me`.
- **CWA** (`calibre.lengoc.me`): thay Calibre-Web, không chạy song song. Cookie `session` riêng trên `calibre.lengoc.me`. OPDS/KOReader trỏ về `https://calibre.lengoc.me/opds`.

## 2. Thứ tự thực hiện

1. Newsman: xác nhận ai được phân giải và ai được mở (mặc định LAN + `netbird`), rồi thêm site block, `caddy validate`, `caddy reload`, kiểm `curl --resolve ... 10.20.0.3`, sau đó thêm bản ghi `records.yaml` và `dnsctl.py plan/apply` cả hai site.
2. CWA: tạo `calibre.lengoc.me` → `10.20.2.1:8083`, kiểm đăng nhập, `/opds`, tải sách, KOReader. `ebooks.lengoc.me` vẫn giữ nguyên.
3. Library: đổi upstream `ebooks.lengoc.me` → `10.20.2.1:8463`, kiểm `302 /login.html`, mở EPUB/PDF, nghe thử, dữ liệu Sổ tay còn nguyên. Soak 7 ngày, không xoá `calibre-web`.

Hoàn tác: đổi upstream `ebooks.lengoc.me` về `10.20.2.1:8083`, xoá site block mới, xoá bản ghi trong `records.yaml`, `dnsctl apply`.

## 3. Cần chốt trước khi làm

- **Tên domain Newsman:** `unified-access.md` có hai chỗ khác nhau: `news.lengoc.me` (dòng 9) và `newsman.lengoc.me` (dòng 66). Dòng 66 là quyết định mới hơn, nên dùng `newsman.lengoc.me`. Cần chủ dự án xác nhận.
- **Edge cho Library/CWA:** `cutover-ebooks.md` ghi "Caddy hoặc NPM" tùy site. Chưa kiểm proxy host hiện tại của `ebooks.lengoc.me` (`ebooks-stack-audit.md` ghi là suy luận, chưa kiểm trực tiếp).
- **Phạm vi:** chỉ bản ghi nội bộ. Không tạo bản ghi Cloudflare công khai.
- **Cloudflare:** connector `cloudflare-api-token` đang lỗi 403 (thiếu scope), nên DNS-01 và thao tác phía Cloudflare chưa làm được cho đến khi sửa token.
- **`records.yaml`:** repo `automations/internal-dns` trả 404 khi đọc qua Gitea, chưa xác định được đường dẫn hoặc quyền truy cập. Cần kiểm trước khi thêm bản ghi.

## 4. Kiểm sau khi làm (đủ mới coi là xong)

- `curl --resolve newsman.lengoc.me:443:10.20.0.3 https://newsman.lengoc.me/` → 200/302, `ssl_verify_result=0`
- `getent hosts newsman.lengoc.me` → `10.20.0.3` trên cả hai site
- `https://calibre.lengoc.me/` 200, đăng nhập được, `/opds` hợp lệ
- `https://ebooks.lengoc.me/` → 302 `/login.html` (sau bước 2), không còn là Calibre-Web
- Từ ngoài Internet: không có bản ghi công khai (NXDOMAIN)

## 5. Nguồn

- `D:\Python\projects\lnc-newsman\.docs\domain-steps.md`
- `D:\Python\projects\lnc-library\.docs\cutover-ebooks.md`
- `D:\Python\projects\lnc-library\.docs\unified-access.md`
- `D:\Python\projects\lnc-library\.docs\ebooks-stack-audit.md`
- Skill: `lnc-internal-domain-config`, `lnc-caddy-fleet`
