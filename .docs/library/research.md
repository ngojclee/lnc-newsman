# Kho sách truyện, ebook và sách nói — nghiên cứu (11/11/2026)

Trạng thái: **nghiên cứu, chưa triển khai**. Chưa đổi gì trên stack `ebooks`.

## 1. Hiện trạng

- `ebooks.lengoc.me` đang chạy **Calibre-Web** (màn hình đăng nhập xác nhận). Chưa đọc được phiên bản, image và volume: trình duyệt của phiên này không mở được Portainer `10.20.2.1:19943` do chứng chỉ tự ký, và em không đăng nhập hộ vào Calibre-Web.
- Tài sản nội bộ liên quan: **PolyVox** (`polyvox.lengoc.me`, dịch + TTS), **LNC StoRan** (`storan.lengoc.me`, viết truyện, có TTS chương và xuất bản), **lnc-notebooklm**.

## 2. LaziBook (book.lazidi.vn) — học được gì

LaziBook là **app gốc iPhone/iPad/Mac** (Android thử nghiệm), không phải web app. Mọi thứ chạy trên máy người dùng, đồng bộ qua iCloud. Không có mã nguồn công khai; tên, biểu tượng, giao diện, bìa và âm nền thuộc nhóm phát triển (điều khoản sử dụng). Vì vậy **không lấy giao diện/tài sản của họ**, nhưng tính năng và các thành phần mở họ dùng thì học được.

| Nhóm | Tính năng của LaziBook | Cách mình làm tương đương |
|---|---|---|
| Nghe | Bấm Nghe từ đúng chỗ đang đọc; tô sáng câu và từng tiếng; trang tự lật; chạm chữ để đọc từ đó | Trình đọc web + TTS phía máy chủ trả mốc thời gian theo câu |
| Nghe | Giọng riêng cho lời thoại; sửa cách đọc từ/tên riêng; tốc độ 0,75–2×; hẹn giờ; hàng chờ | Từ điển phát âm theo cuốn; phát hiện ngoặc kép; hàng chờ trong trình phát |
| Nghe | Xuất M4B mỗi chương một tệp, có mục lục; âm nền (18 nhạc, 12 khung cảnh tự tổng hợp) | Job nền tạo M4B bằng ffmpeg; âm nền phải tự tạo hoặc dùng nguồn có giấy phép |
| Đọc | Lật trang kiểu sách, cuộn dọc, 6 chủ đề sáng/tối, phông, giãn dòng | Trình đọc EPUB trên web (foliate-js/epub.js), dùng lại Giấy/Sepia/Đêm của Newsman |
| Đọc | Tô sáng 5 màu, ghi chú, Sổ tay gom mọi cuốn; tìm trong sách và cả thư viện, gõ không dấu | Bảng annotation + tìm toàn văn SQLite FTS5 bỏ dấu |
| Đọc | Phiên âm pinyin/Hán-Việt; tra từ; công thức, bảng, hình | Giai đoạn sau; dữ liệu Hán-Việt CC BY-SA |
| Làm sạch | Sửa bảng mã TCVN3, dấu đặt sai, chữ dính; bỏ dòng quảng cáo lặp; soát chính tả theo ngữ cảnh; ghi lại EPUB 3 gọn | Pipeline chuẩn hóa tiếng Việt khi nhập sách, giữ tệp gốc |
| Bìa | Tự dựng bìa theo thể loại, mẫu và bảng màu | Tự vẽ mẫu bìa riêng (SVG) |
| Dịch | Dịch Trung/Anh→Việt trên máy, song ngữ theo đoạn, bảng tên và thuật ngữ theo cuốn, văn phong theo thể loại | Dịch phía máy chủ qua LNC Agent Hub/PolyVox, lưu bảng thuật ngữ theo cuốn |
| Nhập | Tệp, link (Wikisource, Gutenberg, trang truyện), **OPDS** (Calibre, Calibre-Web, Komga, Kavita), Drive/Dropbox; chống trùng theo nội dung | Nhập tệp + OPDS + link; hash nội dung; không vượt đăng nhập/tường phí/DRM |
| Thư viện | Xem theo tác giả/bộ/thể loại; trạng thái đọc; thống kê, mục tiêu ngày; sao lưu | Giống, nhiều người dùng |
| Định dạng | EPUB, PDF, MOBI/AZW3, DOCX, TXT, RTF, MD, FB2, CBZ/CBR/CB7/CBT; không gỡ DRM | Calibre `ebook-convert` làm lõi chuyển đổi |

Thành phần mở LaziBook dùng (trang Ghi công): **VieNeu-TTS v3 Turbo/Nano** (Apache-2.0, giọng Việt 3 miền, clone giọng), sea-g2p (chuẩn hóa/phiên âm tiếng Việt), Matcha-TTS + sherpa-onnx (tiếng Trung), Hy-MT2-1.8B qua llama.cpp (dịch), ONNX Runtime. VieNeu-TTS tự chạy được trên CPU; **chưa kiểm chứng** nó có sẵn API máy chủ hay phải tự bọc.

Ghi chú: LaziBook nối được OPDS, nên chỉ cần bật OPDS trên Calibre-Web là anh dùng ngay LaziBook với kho sách hiện có, không cần viết gì.

## 3. Calibre và các bản để học theo

Theo kết quả tìm kiếm ngày 11/11/2026, em chưa tự chạy thử.

| Dự án | Điểm đáng chú ý |
|---|---|
| Calibre 9.x | 9.0 (01/2026) có giao diện kệ sách, tính năng AI tùy chọn; 9.10 làm lại giao diện content server, cài được dạng PWA; bản mới nhất em tìm thấy là 9.11 (07/2026) |
| Calibre-Web (bản đang chạy) | Phát hành chậm; 0.6.26 (02/2026), chủ yếu sửa lỗi |
| Calibre-Web-Automated | 4.0 (01/2026): tự nhập sách từ thư mục, xử lý sách trùng, trung tâm thống kê, Magic Shelves, OAuth, đồng bộ KOReader, tương thích Calibre 9 |
| Kavita / Komga | Mạnh về truyện tranh, manga, light novel; có OPDS, API |
| BookLore | Giao diện hiện đại nhưng dự án đang xáo trộn (bị fork thành Grimmory 03/2026) |
| Audiobookshelf | Chuyên sách nói, nhớ chỗ nghe; không có OPDS |

## 4. Gộp vào Newsman hay tách riêng

**Đề xuất: tách app riêng, có kết nối.** Tên tạm `lnc-library` (anh đặt lại được), giữ domain `ebooks.lengoc.me`.

- Khác bản chất: Newsman xử lý mục ngắn, tươi theo giờ, chỉ lưu tiêu đề + link. Thư viện lưu tệp lớn lâu dài, có job TTS/chuyển đổi nặng CPU, và có ràng buộc bản quyền khác hẳn.
- Gộp sẽ làm chậm lộ trình tài chính/on-chain (P1.5–P4) vừa duyệt và làm phình một app.
- Dùng chung: design token và 3 chế độ đọc của Newsman, đăng nhập, LNC Agent Hub (LLM), PolyVox (TTS/dịch), StoRan (truyện tự viết xuất sang thư viện), Business MCP Hub.
- Kết nối: Newsman có thể gửi bản tin/báo cáo sang thư viện để thành "số báo" EPUB hoặc bản nghe; hai app gọi nhau qua REST/MCP.

**Với Calibre: không sửa (fork) Calibre.** Giữ thư viện Calibre (`metadata.db` + thư mục sách) làm kho gốc và `ebook-convert` làm lõi chuyển đổi — đó là phần native nên để nguyên theo blueprint. App mình là lớp phía trên: giao diện, trình đọc, sách nói, dịch, API/MCP.

Lộ trình gợi ý:

- **L0** — Sao lưu, nâng stack hiện tại lên Calibre-Web-Automated, bật OPDS. Có ngay tự nhập sách, chống trùng, và dùng được với LaziBook.
- **L1** — App mới chỉ đọc trên cùng thư viện: kệ sách, trình đọc EPUB/PDF, tìm kiếm bỏ dấu, tiến độ đọc. Calibre-Web vẫn chạy song song.
- **L2** — Sách nói: TTS theo chương qua PolyVox/VieNeu-TTS, tô sáng theo câu, xuất M4B, hàng đợi job.
- **L3** — Làm sạch sách tiếng Việt, bìa tự dựng, dịch song ngữ với bảng thuật ngữ.
- **L4** — Nhập từ link/OPDS, kho truyện từ StoRan, API/MCP, đăng ký Hub.

## 5. Ranh giới

- Không gỡ DRM, không vượt đăng nhập/tường phí, không tải hàng loạt từ trang truyện trái điều khoản.
- Sách nói và bản dịch của tác phẩm còn bảo hộ chỉ dùng cá nhân/nội bộ, không phát hành công khai.
- Clone giọng chỉ từ giọng của mình hoặc người đã đồng ý.

## 6. Cần anh quyết định

1. Tách app riêng theo đề xuất, hay vẫn muốn gộp vào Newsman.
2. Có làm L0 (nâng lên Calibre-Web-Automated) không — cần anh cho xem stack (compose, image, volume) vì em chưa đọc được Portainer.
3. Provider TTS chính: PolyVox hiện có, hay tự chạy VieNeu-TTS.
4. Thứ tự so với Newsman P1.5 đang chờ làm.

## Nguồn

- https://book.lazidi.vn/ , /credits , /terms
- https://alternativeto.net/news/2026/6/calibre-9-10-brings-redesigned-content-server-ui-pwa-support-image-compression-and-more/
- https://linuxiac.com/calibre-9-0-e-book-manager-released-with-new-bookshelf-view/
- https://newreleases.io/project/github/crocodilestick/Calibre-Web-Automated/release/v4.0.1
- https://newreleases.io/project/github/linuxserver/docker-calibre-web/release/0.6.26-ls390
- https://selfhosting.sh/best/ebook-readers/
- https://dev.to/productimpossible/5-self-hosted-ebook-managers-worth-trying-in-2026-59hn
- https://github.com/pnnbao97/VieNeu-TTS
