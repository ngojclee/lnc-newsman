# LNC Newsman

Nền tảng đọc và nghiên cứu tin nội bộ, dự kiến hỗ trợ nhiều chủ đề, nguồn tin, báo cáo, cảnh báo và REST/MCP. Hiện có **P0 local, bộ thu thập RSS/Atom của P1 và cây vùng + region pack của P1.5**; chưa nghiệm thu P1 với nguồn thật và chưa triển khai.

## Đã có

- Trang đọc VI/EN với ba chế độ Giấy/Sepia/Đêm; trang **Hôm nay** liệt kê tin mới, trang **Chủ đề** lọc theo chủ đề (gồm cả chủ đề con). Mỗi tin dẫn về bài gốc.
- **Cài đặt → Nguồn & chủ đề**: thêm, sửa, bật/tắt nguồn và cây chủ đề; **Kiểm tra feed** trước khi lưu; **Quét ngay**; **Tiếp tục** nguồn bị tạm dừng; xem lần quét thành công và lỗi gần nhất.
- **Thu thập tin tự động**: công tắc trong Cài đặt, **mặc định tắt**. Khi bật, nguồn đang bật được quét theo khoảng đã đặt; cùng một máy chủ cách nhau ít nhất 5 giây; lỗi thì lùi lịch, lỗi 5 lần liên tiếp thì tự tạm dừng.
- Chỉ lưu tiêu đề, đường dẫn, mã định danh và ngày đăng; không lưu mô tả hay nội dung bài.
- Bộ tải chặn các dải địa chỉ đã liệt kê trong mã (loopback, private, link-local, CGNAT/NetBird, multicast, dành riêng cho tài liệu, reserved, IPv4-mapped…) sau khi phân giải DNS và ở mỗi bước chuyển hướng; chỉ cho cổng 80/443/8080/8443; giới hạn 5 MB và 25 giây. **Đây không phải allowlist đầy đủ**: địa chỉ công khai của mạng nội bộ anh (nếu có) vẫn lọt, và các dải mới chưa được bổ sung phải sửa mã. ETag/Last-Modified chỉ gửi lại cho đúng origin đã cấp chúng.
- Khóa quản trị qua biến môi trường, phiên ký HMAC có thu hồi; API key riêng chỉ đọc (`/api/status`, `/api/v1/topics`, `/api/v1/articles`).
- **P1.5 — vùng và region pack**: cây `global` (gốc ảo, luôn bật) › `eu` › `fr`, `us`, `vn`. Pack là thư mục trong `REGIONS_DIR` (mặc định `<DATA_DIR>/regions`); bản mặc định nhúng trong binary và chỉ ghi ra khi thư mục chưa có `region.toml`. Quét khi khởi động và nút **Nạp lại vùng** chỉ *phát hiện*: vùng và nguồn mới luôn ở trạng thái **tắt**; chỉ job và truy cập mạng sau khi admin bật trong UI. Lỗi từng file (cú pháp, mã trùng/lệch, cha thiếu, chu trình, URL sai, symlink) được báo trong Cài đặt và giữ nguyên cấu hình hợp lệ lần trước.
- **Bộ chọn vùng** trên thanh trên (chọn Pháp hiển thị cả EU và Toàn cầu); `/api/articles` và `/api/v1/articles` nhận tham số `region`; scheduler chỉ chạy nguồn thuộc vùng đang bật (gồm vùng cha).
- Nguồn do pack quản lý chỉ bật/tắt trong UI; nguồn thêm tay vẫn chọn vùng như thường.
- Dockerfile và cấu hình Portainer **dự thảo, chưa build/test**.

### P2 nền tảng (mới, chưa có nhà cung cấp thật)

- **Kho secret**: khóa nhà cung cấp được mã hóa XChaCha20-Poly1305 bằng `LNC_MASTER_KEY` (64 hex hoặc 32 byte base64), nonce mới mỗi lần ghi, tên khóa ràng buộc vào ciphertext. Chỉ ghi — API/UI không bao giờ trả lại giá trị. Xoay khóa bằng `LNC_MASTER_KEY_OLD` + nút rewrap; nếu một dòng không mở được thì không dòng nào bị ghi đè. Chưa đặt khóa ⇒ kho đóng (503), không lưu plaintext.
- **Kho chuỗi số liệu**: `series` + `observations` (kỳ dữ liệu, thời điểm biết `as_of`, `revision`, chất lượng, nguồn) + sự kiện. Sửa số liệu là thêm revision; truy vấn theo `as_of` tái hiện đúng những gì đã biết.
- **Khung connector**: trait `Connector` với cổng do khung áp (connector bật, vùng bật, secret hợp lệ, hạn mức ngày, cache, lịch); connector bắt buộc dùng bộ tải an toàn của hệ thống. Mới có connector **giả lập** để test trên loopback.
- **UI**: trang Thị trường (trạng thái rỗng trung thực), khối quản trị Khóa nhà cung cấp và Connector.
- **Connector thật (11/11/2026)**, theo bộ nguồn anh chọn: mempool.space, ECB Data Portal, Federal Register (không khóa); FRED và CoinGecko Demo (cần khóa anh tự tạo trong **Cài đặt → Khóa nhà cung cấp**); SEC EDGAR (cần `SEC_USER_AGENT` do anh điền). Tất cả **mặc định tắt**, hạn mức đếm theo từng yêu cầu HTTP, mọi giá trị kèm dòng ghi nguồn theo điều khoản. Pack `vn` có nguồn VnExpress (đang **tắt**), kèm ghi chú điều khoản.
- **Chưa nghiệm thu với nhà cung cấp thật.** Kịch bản từng bước (số yêu cầu, điều khoản cần đọc, tiêu chí đạt, cách dừng) nằm ở [.docs/live-acceptance.md](.docs/live-acceptance.md) và **chờ anh duyệt riêng**.
- **Chưa có**: lịch chạy tự động cho connector, biểu đồ, lưu filing SEC (mới đếm), chứng khoán VN/Pháp, ETF Farside, Légifrance.

### P3 nền tảng — văn bản chính thức và thuế (offline)

- Pack có thêm `official.toml` (cơ quan ban hành) và `tax.toml` (nguồn thuế + hạn nộp định kỳ) cho cả 5 vùng; kiểm tra từng file, mọi mục **mặc định tắt**.
- **Văn bản**: tìm kiếm bỏ dấu tiếng Việt (`thue` khớp `thuế`); tóm tắt tự viết luôn kèm dòng "không phải tư vấn pháp lý/thuế".
- **Lịch thuế**: tính hạn theo tháng/quý/năm, múi giờ từng vùng, dời theo cuối tuần đúng như pack khai báo; **chưa tính ngày lễ công cộng**. API `/api/v1/tax-calendar`; bật/tắt từng rule trong quản trị.
- **UI** trang **Vùng**: tin + văn bản + lịch thuế theo vùng đang bật, 3 chế độ đọc, điện thoại 375 px.
- ⚠️ **Hạn nộp trong pack là dữ liệu mẫu** (`verified = false`), chỉ để kiểm máy tính hạn; **chưa đối chiếu luật từng vùng**. Cần anh xác minh với cơ quan thuế rồi sửa ngày và đặt `verified = true` trước khi dùng thật. Chưa gửi Telegram/email (P7).

### P5 nền tảng — phân loại và lớp LLM (offline)

- **Luật trước, LLM sau**: mỗi bài mới được gắn vùng/chủ đề/thực thể bằng luật ngay khi thu thập (không gọi mạng). Nhãn lưu kèm nguồn gốc; nhãn do người sửa không bị ghi đè.
- **Lớp LLM mặc định tắt**: cần `LLM_BASE_URL`, `LLM_MODEL` và khóa `llm-api-key`; hạn mức token/ngày kiểm trước khi gọi; cache theo hash; lỗi ⇒ giữ nhãn luật. API `GET/PATCH /api/admin/llm`.
- **Đo trên 50 mục mẫu**, tách 25 dev / 25 holdout: **dev 96%, holdout 84%** (ngưỡng 80%). Báo cáo chỉ dùng con số holdout làm bằng chứng.
- Chưa làm: đo với LLM thật, tóm tắt, gom story, hiện nhãn trên trang đọc.

### P4 filings + lớp provider Hub (offline, 11/11/2026)

- **Báo cáo tài chính & hồ sơ niêm yết**: bảng `companies`/`filings`/`financial_facts` (chỉ metadata + số liệu + link, có as-of + revision); connector `sec-edgar-filings` cho 10-K/10-Q/8-K/S-1 và XBRL, mặc định **tắt**, cần `SEC_USER_AGENT`.
- Điều khoản từng nguồn ở [.docs/sources-candidates.md](.docs/sources-candidates.md) mục 7: EDGAR mức A; AMF mức B/C; ESAP chưa hoạt động; Việt Nam mức C.
- **LNC Agent Hub**: lớp provider chung cho LLM (OpenAI-compatible, alias model theo tác vụ) và agent nghiên cứu; khóa riêng nhập ở Cài đặt (chỉ ghi), **allowlist host nội bộ theo tên** (mặc định vẫn chặn mọi địa chỉ riêng), hạn mức token + cache + timeout; nút **Kiểm tra kết nối** chỉ liệt kê model. Hướng dẫn phía Hub: [.docs/hub-integration.md](.docs/hub-integration.md).
- ⚠️ **Chưa gọi Hub thật, chưa nhập khóa, chưa đăng nhập Hub admin.** Toàn bộ test dùng Hub giả lập loopback.

## Chưa có

Nghiệm thu P1 với ≥20 nguồn RSS thật, nghiệm thu P1.5 (danh sách nguồn thật theo vùng) và nghiệm thu P2 với nhà cung cấp thật; các file pack `official.toml`/`tax.toml`/`markets.toml`/`macro.toml` thuộc P2–P4. Chưa có tải trang HTML/robots.txt, `lnc-scrapling`, proxy, LLM, Agent Hub, tài khoản người đọc, báo cáo, cảnh báo, MCP, quản lý khóa qua UI và kho secret mã hóa. Nguồn mẫu trong pack trỏ tới `example.invalid` (tên miền dành riêng), **không phải nguồn đã kiểm chứng**.

## Chạy local trên Windows

Cần Rust 1.93+, Node 22+, npm; lệnh build từ thư mục `ui`:

```bash
npm ci && npm run build
```

Từ thư mục gốc dự án, mở `run-local.cmd`. Script dùng khóa demo `dev-only-key`, chỉ bind `127.0.0.1` và dùng HTTP; không dùng script/key demo để triển khai. Sau đó mở `http://127.0.0.1:8000/login.html`. `COOKIE_SECURE=true` là mặc định an toàn cho HTTPS; local HTTP tắt riêng trong script.

```bash
./run-local.cmd
```

Kiểm tra backend:

```bash
cargo test --offline
```

Region pack (P1.5) nằm trong `DATA_DIR/regions` (đổi bằng `REGIONS_DIR`). Lần chạy đầu, nếu thư mục chưa có `region.toml`, ứng dụng ghi bộ pack mặc định gồm `global`, `eu`, `eu/fr`, `us`, `vn`; các file đã có **không bị ghi đè**. Nguồn mẫu trỏ `example.invalid` và đều ở trạng thái tắt — hãy thay bằng nguồn đã kiểm chứng rồi mới bật vùng và nguồn trong **Cài đặt → Vùng và region pack**.

## Ranh giới triển khai

Đích dự kiến là Portainer `10.20.2.1`, bind root `/share/Docker`; cổng `8462` mới là **đề xuất**, chưa giữ chỗ hoặc kiểm tra trên host. Cấu hình [stack-image.yml](docker/stack-image.yml) chỉ là mẫu: chưa có image, chưa kiểm tra CPU NAS, chưa thử container hoặc cấu hình Caddy/DNS/Hub. Không dùng cho production trước khi hoàn tất các bước xác thực và được duyệt. Cookie HTTPS chỉ hoạt động nếu reverse proxy cung cấp HTTPS và `COOKIE_SECURE=true`.

Giới hạn đăng nhập đếm theo địa chỉ kết nối TCP và chỉ lưu trong bộ nhớ của một process. Sau reverse proxy, nhiều người có thể dùng chung hạn mức; cơ chế trusted proxy/distributed rate limit, CSP review, audit và kiểm thử bảo mật vẫn cần hoàn thiện trước production.

Xem [API_DOCS.md](API_DOCS.md), [plan](.docs/plan.md) và [project status](.docs/PROJECT_STATUS.md).
