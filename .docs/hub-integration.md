# Nối Newsman với LNC Agent Hub — việc chủ dự án cần làm

Trạng thái: **Newsman đã có lớp provider sẵn sàng; chưa gọi Hub thật.** Mọi test dùng Hub
giả lập loopback. File này là danh sách việc **phía Hub admin** để khi anh bật thì đúng ngay,
không phải dò lại.

Địa chỉ Hub dự kiến: `https://agents-hub.lengoc.me`. Tài liệu này **không** đăng nhập vào Hub,
không tạo/đọc khóa hộ, không gọi Hub thật.

## 1. Việc ở Hub admin (chủ dự án làm)

1. **Tạo khóa riêng cho Newsman** (không dùng khóa của app khác):
   - Đặt tên dễ nhận, ví dụ `newsman-prod` hoặc `newsman-local`.
   - Quyền tối thiểu cần cho P5/P6 hiện tại:
     - `chat.completions` (hoặc quyền tương đương gọi model),
     - `models.read` (để nút "Kiểm tra kết nối" liệt kê model),
     - `agents.run` **chỉ cho** agent `news-analyst-argus` nếu bật phần nghiên cứu.
   - **Không** cấp quyền quản trị, không cấp quyền tạo khóa, không cấp quyền đọc khóa khác.
2. **Đặt alias model** cho từng tác vụ, rồi ghi lại đúng chuỗi Newsman sẽ dùng:
   - `HUB_MODEL_CLASSIFY` — model rẻ nhất đủ phân loại (vd. một model mini).
   - `HUB_MODEL_SUMMARISE` — model tầm trung.
   - `HUB_MODEL_ANALYSE` — model mạnh cho phân tích/nhận định.
   - Alias phải **tồn tại trên Hub**; nếu sai, nút kiểm tra vẫn liệt kê model nhưng lần gọi
     phân loại sẽ lỗi `provider_error`.
3. **Đặt hạn mức token/ngày cho khóa của Newsman** ở phía Hub (nếu Hub hỗ trợ), độc lập với
   hạn mức nội bộ của Newsman. Hai lớp hạn mức là cố ý: Newsman tự dừng trước, Hub là chốt cuối.
4. **Xác nhận agent id** trong catalog: mặc định Newsman dùng `news-analyst-argus`. Nếu Hub đổi
   id, đặt `HUB_AGENT_ID`.
5. **Giọng đọc `polyvox-11labs`** thuộc Library, không cấp cho Newsman; không đưa vào danh sách
   quyền của khóa Newsman.

## 2. Việc ở Newsman (biến môi trường, không nhập qua UI)

| Biến | Ý nghĩa | Ví dụ |
|---|---|---|
| `HUB_BASE_URL` | gốc API OpenAI-compatible của Hub | `https://agents-hub.lengoc.me/api/v1` |
| `HUB_ALLOWED_HOSTS` | danh sách host **được phép** phân giải ra địa chỉ nội bộ, cách nhau dấu phẩy | `agents-hub.lengoc.me` |
| `HUB_MODEL_CLASSIFY` / `HUB_MODEL_SUMMARISE` / `HUB_MODEL_ANALYSE` | alias model theo tác vụ | `mini`, `default`, `strong` |
| `HUB_AGENT_ID` | agent cho tác vụ nghiên cứu | `news-analyst-argus` |
| `HUB_TIMEOUT_SECONDS` | timeout mỗi lần gọi (5–300) | `60` |
| `LLM_DAILY_TOKEN_BUDGET` | hạn mức nội bộ mỗi ngày | `50000` |

**Khóa Hub không nằm trong biến môi trường.** Nó là secret `hub-api-key`, nhập tại
**Cài đặt → LNC Agent Hub** (ô chỉ ghi, không hiện lại), mã hóa trong kho secret của Newsman.

### Vì sao cần `HUB_ALLOWED_HOSTS`

Newsman mặc định chặn mọi địa chỉ nội bộ để chống SSRF. Hub của anh chạy trong LAN nên nếu
không khai báo, request sẽ bị chặn (`blocked_address`). Cơ chế hiện tại:

- **Chỉ host được nêu đúng tên** mới được phép phân giải ra địa chỉ riêng; không có wildcard,
  không mở cho cả dải mạng.
- Host không nằm trong allowlist vẫn bị chặn như cũ, kể cả `127.0.0.1`.
- Quyền này chỉ gắn với bộ tải dùng cho Hub; bộ tải cho RSS/connector **không** bị ảnh hưởng.

Nếu Hub có tên miền công khai và chứng chỉ hợp lệ, có thể bỏ allowlist; địa chỉ công khai vẫn
qua được lớp kiểm tra.

## 3. Kiểm tra kết nối (chỉ khi anh bấm)

Nút **Kiểm tra kết nối** trong Cài đặt gọi **duy nhất** `GET {HUB_BASE_URL}/models` với khóa
Bearer. Nó:

- không sinh nội dung, không tốn token;
- chạy **chỉ** khi admin bấm (không có scheduler, không tự chạy lúc khởi động);
- trả về danh sách model để anh đối chiếu với alias đã đặt.

Thành công đúng nghĩa: thấy ≥ 3 model và danh sách chứa các alias anh định dùng ở mục 1.2.
Nếu 401/403: khóa sai hoặc thiếu quyền `models.read`. Nếu `provider_host_not_allowed`: thêm
host vào `HUB_ALLOWED_HOSTS`. Nếu `provider_timeout`: tăng `HUB_TIMEOUT_SECONDS` hoặc kiểm
đường mạng LAN/NetBird.

## 4. Ranh giới dữ liệu

- Newsman chỉ gửi: tiêu đề bài báo, và (khi làm P6) số liệu đã lưu trong kho. **Không** gửi
  nội dung người dùng, khóa khác, hay dữ liệu cá nhân.
- Không gửi bài của nguồn có điều khoản cấm xử lý bằng dịch vụ bên thứ ba; danh sách nguồn cần
  anh rà theo `.docs/sources-candidates.md`.
- Kết quả LLM được lưu kèm model và phiên bản prompt để truy vết; xóa cache bằng cách xóa bảng
  `llm_cache` (không ảnh hưởng nhãn đã lưu).

## 5. Khi Hub không sẵn sàng

- Chế độ **không LLM** là mặc định và luôn chạy: nhãn do luật, không phụ thuộc Hub.
- Provider lỗi ⇒ hàm trả lỗi, caller giữ nhãn luật. Không có đường nào để Hub chết làm hỏng
  trang đọc.
- Tắt/bật từng provider: xóa `HUB_BASE_URL` (tắt hoàn toàn) hoặc xóa khóa `hub-api-key`
  (giữ cấu hình, không gọi được).

## 6. Chưa kiểm chứng

- Chưa gọi Hub thật lần nào; toàn bộ test dùng Hub giả lập trên `127.0.0.1`.
- Chưa biết Hub có đúng các route `/chat/completions`, `/models`, `/agents/{id}/run` hay không.
  Nếu Hub dùng route khác, chỉ cần sửa ba hằng đường dẫn trong `src/hub.rs`; **không** cần đổi
  lớp bảo mật hay hạn mức.
- Chưa đối chiếu định dạng trả về của `/agents/{id}/run` (`output` hay `text`); connector chấp
  nhận cả hai và từ chối nếu không có nội dung.

---

# Phần ngược lại: đưa `/mcp` của Newsman lên Business MCP Hub (v0.4.4)

Chiều này khác hoàn toàn chiều trên. Trên đây Newsman **gọi** Hub; dưới đây **Hub gọi
vào** `/mcp` của Newsman. Hub là MCP-native: nó mở session MCP tới `/mcp` của app và
proxy `tools/call`, nên phía Newsman chỉ cần một **ranh giới xác thực + scope**, không
phải thêm API.

## 1. Tạo bản ghi trên Hub (chủ dự án)

`POST /admin/api/connectors` với token `BUSINESS_MCP_ADMIN_TOKEN`:

| Trường | Giá trị |
|---|---|
| `id` | `lnc-newsman` |
| `display_name` | `LNC Newsman` |
| `namespace` | `newsman` |
| `mcp_url` | `https://newsman.lengoc.me/mcp` |
| `health_url` | `https://newsman.lengoc.me/health` |
| `audience` | `lnc-newsman` |
| `scopes` | `["newsman:read"]` |
| `enabled` | `false` |

Bản ghi được tạo ở trạng thái **tắt** và tự động ghép cặp. Sau đó
`POST /admin/api/connectors/lnc-newsman/pairing` trả `connector_token` +
`delegation_secret` **một lần duy nhất** — hai giá trị này không bao giờ được in ra
chat hay commit.

## 2. Đặt hai giá trị đó vào Newsman — chọn một trong hai

**Cách A — Portainer env (deploy-time).** Stack 108 → Environment variables:

```env
CONNECTOR_BEARER=<connector_token>
DELEGATION_SECRET=<delegation_secret>
```

Redeploy để container nhận env mới.

**Cách B — `/admin` của Newsman (không cần redeploy).** Cài đặt → *Khóa nhà cung cấp*,
lưu hai tên `connector-bearer` và `delegation-secret`. Chúng vào kho mã hóa sẵn có
(`LNC_MASTER_KEY`), và có hiệu lực **ngay** trên tiến trình đang chạy.

**Owner không cần đặt env nào cho Newsman.** Cả ba giá trị đều khai trong compose dạng
**rỗng**, và rỗng nghĩa là "để code quyết định": hai khóa rơi xuống giá trị dán ở `/admin`,
còn trần scope lấy từ chính build (`newsman:read` + `newsman:write`). Không cần redeploy để
"ăn" env. Chỉ đặt `CONNECTOR_MAX_SCOPES` nếu muốn trần **hẹp hơn** mặc định.

Nguyên tắc đằng sau (đáng nhớ khi sửa sau này): **giá trị mặc định nằm ở tầng nào thì hiệu
lực giới hạn ở tầng đó.** Mặc định trong code đúng với mọi deployment; mặc định trong
`stack.yml` chỉ đúng với deployment đọc file đó — và có thể âm thầm hạ trần thấp hơn thứ
code cho phép.

Quy tắc `env_or_cfg`: **env thắng** khi có giá trị; env trống thì dùng giá trị trong
`/admin`. Vì vậy có thể ghép nối ở `/admin` trước rồi chuyển sang env sau mà không phải
xoá gì. Nếu để cả hai trống, connector **chưa ghép nối** và mọi call trả 401 — cố ý
không phân biệt được với "sai khóa".

## 3. Bật và nghiệm thu

1. `POST /admin/api/connectors/lnc-newsman/test` → mong `{status:"ok",
   mcp_initialize:"ok", tool_count:7}`.
2. `PUT /admin/api/connectors/lnc-newsman/state` body `{"enabled":true}` (PUT, không POST).
3. Thử thật qua Hub bằng một agent tạm có grant `newsman:read`, gọi
   `newsman__latest_articles`; xong thì `POST /admin/api/clients/{id}/revoke`.

## 4. Vì sao tên tool trên Hub là `newsman__latest_articles` và trên `/mcp` là `latest_articles`

Hub tự thêm namespace. Vì vậy `/mcp` của Newsman công bố **tên trần**; nếu công bố
`newsman__latest_articles` thì agent sẽ thấy `newsman__newsman__latest_articles`. Tên có
tiền tố vẫn được **chấp nhận** khi gọi (strip trước khi tra) để client viết cho 0.4.3
không gãy.

## 4b. Scope ghi (`newsman:write`, từ 0.5.0)

Agent được cấp `newsman:write` **và** `newsman:read` mới ghi được, và chỉ ghi dữ liệu
phân tích của Newsman:

| Tool | Ghi gì |
|---|---|
| `note_create` / `note_update` / `note_delete` | ghi chú trên một bài hoặc một tài liệu |
| `analysis_create` / `analysis_update` | brief/tổng hợp, có danh sách id bài nguồn |
| `article_flag` | nhãn ngắn trên một bài |

**Không** mở qua connector: cấu hình nguồn/vùng, connector, khoá provider, bật collector,
gửi alert ra ngoài — vẫn chỉ qua `/admin`.

Bốn điều đáng biết khi vận hành:

1. **Tác giả luôn là `client_id` của Hub**, không phải tham số. Gửi `author` trong
   arguments sẽ bị từ chối (`unknown argument`), không bị bỏ qua — nếu bỏ qua im lặng,
   agent sẽ tưởng đã đặt được.
2. **Sửa/xoá chỉ trên bản ghi của chính mình.** Ghi chú owner viết trong web UI
   (`author_client` NULL) và ghi chú của agent khác đều không sửa được.
3. **Mỗi lần ghi có một dòng `newsman_write_log`** (thời điểm, client_id, tool, id bản
   ghi) — xem ở `/admin` card connector hoặc qua tool `write_log`.
4. **Trần scope**: `CONNECTOR_MAX_SCOPES` mặc định giờ là `newsman:read,newsman:write`.
   Muốn connector chỉ đọc thì đặt lại `newsman:read`.

## 5. Những gì phía Newsman từ chối và mã lỗi

| Tình huống | HTTP | `error` |
|---|---|---|
| Không có credential | 401 | `unauthorized` |
| Có bearer connector nhưng sai/connector chưa ghép nối | 401 | `connector_auth_required` |
| Thiếu header delegation, sai chữ ký, sai audience | 403 | `delegation_denied` |
| Timestamp lệch quá 90 s | 403 | `delegation_denied` |
| Nonce đã dùng trong 300 s | 403 | `delegation_denied` |
| Scope ngoài trần | 403 | `delegation_denied` |
| Tool không tồn tại **hoặc** thiếu scope | 200 (JSON-RPC) | `-32601` + `Unknown or unauthorized MCP tool` |

Có hai scope: `newsman:read` và `newsman:write`. `CONNECTOR_MAX_SCOPES` từ chối mọi giá trị
khác thay vì âm thầm mở rộng.

`POST /mcp` được miễn header `X-Requested-With` khi request mang bearer connector (Hub
không gửi được header đó, và request mang bearer không có quyền cookie để mượn). Mọi
route khác giữ nguyên CSRF.
