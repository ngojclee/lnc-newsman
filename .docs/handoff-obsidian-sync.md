# Bàn giao — Newsman: hai thay đổi để đồng bộ hai chiều với Obsidian

> Dán cả file này vào phiên làm việc mới trên repo `D:\Python\projects\lnc-newsman`.
> Soạn 2026-11-12.

## Trạng thái hiện tại (đã kiểm trên production)

- App Rust, production `https://newsman.lengoc.me`, version **0.5.0**.
- MCP endpoint `POST /mcp`, code ở `src/mcp.rs`. **16 tool**:
  - đọc: `regions`, `topics`, `latest_articles`, `documents`, `tax_calendar`, `series`,
    `observations`, `notes`, `analyses`, `write_log`
  - ghi: `note_create`, `note_update`, `note_delete`, `analysis_create`, `analysis_update`,
    `article_flag`
- `src/connector.rs` **đã xong**: xác thực chữ ký Business MCP Hub qua bộ header
  `X-MCP-Hub-Client-{ID,Scopes,Timestamp,Nonce,Signature}`. **Không sửa file này.**
- Ghi chỉ chạm 3 bảng: `newsman_notes`, `newsman_analyses`, `newsman_article_flags`
  (migration 18). Tác giả do Hub đặt qua `author_client`, **không nhận từ tham số**.

## Phần KHÔNG phải việc code (đừng làm)

Tạo bản ghi connector trên Business MCP Hub là **thao tác UI của chủ dự án**. Đừng viết script đăng ký
connector, đừng gọi Admin API của Hub, đừng tìm token. App đã sẵn sàng chờ cấu hình ở `/admin`.

---

## Bối cảnh

`lnc-obsidian` có tool `library_note_sync` ghi vào vault Obsidian theo cơ chế **hai vùng**:

```markdown
<!-- LNC:generated -->
## Dự báo (máy tính)
... máy sinh, máy ghi đè được ...
<!-- LNC:generated:end -->

## Ý kiến của anh
(người gõ ở đây, máy KHÔNG được ghi đè)
```

Vòng mong muốn: Newsman sinh dự báo → Obsidian hiển thị → anh viết ý kiến ở vùng dưới → ý kiến **quay
ngược về Newsman** thành phản hồi gắn với đúng bài phân tích.

Thiếu đúng một chỗ nối: hiện `note_create` chỉ nhận `article_id` hoặc `document_id`, chưa gắn được vào
`analysis`.

## Việc 1 — đọc: cho phía gọi lấy được danh sách analysis để sync

Tool `analyses` **đã có** và trả gần đủ: `id`, `kind`, `title`, `topic_slug`, `region_code`, `body`,
`source_article_ids`, `created_at`, `updated_at` (`src/write.rs:435`).

Cần thêm **hai thứ**:

1. **`stable_key`** = `"analysis-<id>"` trong mỗi phần tử. Ổn định qua mọi lần sửa nội dung — Obsidian
   dùng nó làm tên file, đổi key là mất đường dẫn phản hồi.
2. **Tham số `updated_since`** (ISO date) để sync tăng dần, lọc theo `updated_at`.

Bạn có thể làm điều này bằng cách **mở rộng tool `analyses`** hoặc tạo tool riêng
`analysis_feedback_targets` — tuỳ bạn thấy hợp lý hơn với code hiện có. Miễn `stable_key` ổn định và
`updated_since` lọc đúng.

## Việc 2 — ghi: cho `note_create` nhận `analysis_id`

Thêm lựa chọn thứ ba cạnh `article_id` / `document_id`. **Đúng một trong ba** phải được truyền.

### ⚠️ Cảnh báo kỹ thuật quan trọng

`migrations/0018_agent_write.sql` có ràng buộc:

```sql
CHECK ((article_id IS NOT NULL) <> (document_id IS NOT NULL))
```

Thêm cột `analysis_id` **bắt buộc phải sửa cả CHECK**, và **SQLite không ALTER được constraint**.
Phải **rebuild bảng**: tạo bảng mới với cột + CHECK mới → copy dữ liệu → drop bảng cũ → rename. Làm
theo đúng cách repo này đang làm migration (xem các migration trước), **không sửa migration cũ**.
Kiểm bằng cách chạy trên DB đã có dữ liệu và restart hai lần (migration phải idempotent hoặc có guard).

### Ràng buộc an toàn — không thương lượng

1. **Không cho sửa/xoá analysis qua MCP.** Chỉ đọc. Phản hồi là note, không phải ghi đè phân tích.
2. **`body` tối đa 4000 ký tự** như `note_create` hiện tại — giữ nguyên giới hạn cho nhánh mới.
3. **Author do Hub đặt**, không nhận từ tham số (đúng như hiện tại).
4. **`analysis_id` phải tồn tại** — không tồn tại thì trả lỗi rõ ràng, không tạo note mồ côi.
5. Mọi write vẫn phải đi qua `newsman_write_log`.
6. Sửa/xoá note vẫn chỉ trên bản ghi của chính agent (giữ nguyên như hiện tại).

## Kiểm chứng

1. `cargo test` — toàn bộ pass, gồm docs-test phủ tên tool trong `API_DOCS.md`.
2. Test mới:
   - `stable_key` đúng định dạng và **không đổi giữa hai lần gọi**
   - `updated_since` lọc đúng
   - `note_create` với `analysis_id` hợp lệ → tạo được note
   - `note_create` với `analysis_id` không tồn tại → lỗi rõ ràng
   - truyền cả `article_id` và `analysis_id` → bị từ chối
   - migration chạy lại trên DB đã có dữ liệu không lỗi
3. Chạy thật trên production (**chỉ đọc** cho phần đọc), che `body`:
   ```
   POST https://newsman.lengoc.me/mcp
   Authorization: Bearer <LNC_API_KEYS>
   {"jsonrpc":"2.0","id":1,"method":"tools/call",
    "params":{"name":"analyses","arguments":{"limit":3}}}
   ```
4. Cập nhật `API_DOCS.md` và `.docs/PROJECT_STATUS.md`.

## Báo cáo khi xong

Files đã sửa · `cargo test` · cấu trúc kết quả gọi thật (đã che nội dung) · mô tả migration mới nếu có
(cột mới, cách rebuild bảng) · **không tự deploy** — báo để chủ dự án quyết định.

## Lưu ý khi deploy

NAS build Docker **tuần tự**, mỗi lượt ~13–15 phút, và **mỗi push vào `main` kích một lượt build**.
Gộp thay đổi vào ít commit rồi push một lần — hàng đợi dùng chung với Library và Obsidian.
