# Mobile / APK — kế hoạch (CHƯA build)

Trạng thái: **chỉ kế hoạch**, không có APK, không có project Capacitor/Tauri nào
được tạo. Viết 2026-11-12; cập nhật cùng ngày để **dùng chung một phương án với
LNC Library** (`lnc-library/.docs/mobile-apk-plan.md`), vì hai app là một họ giao
diện và chủ dự án chỉ nên phải quyết một lần.

Mọi con số dưới đây là **đề xuất**, chưa đo trên máy thật. Không có bước nào trong
tài liệu này được thực hiện.

---

## 0. Tóm tắt phương án — phải giống bên Library

| Câu hỏi | Phương án chọn (cả hai app) |
|---|---|
| Đóng gói | **Capacitor 7** trên SPA React/Vite sẵn có |
| Đăng nhập trong app | **Địa chỉ máy chủ** trước, rồi xác thực bằng **bearer token chỉ-đọc** (`LNC_API_KEYS`) |
| Cookie cross-site | **Không** — không nới `SameSite=None`, không thêm CORS cho WebView |
| HTTP LAN trên Android | chỉ mở cleartext cho dải nội bộ đã biết, qua `network_security_config` |
| iOS | **PWA trước** (đã có manifest + `apple-touch-icon`); native iOS chỉ khi có máy Mac |
| Quản trị trong app | **Không** trong v1 (chỉ đọc); quản trị giữ trên web |

Nếu Library chọn khác ở bất kỳ dòng nào, sửa bảng này và file tương ứng bên
Library **trong cùng một lần** — đừng để hai app lệch nhau.

---

## 1. Cái đã có trong UI web (điều kiện cần trước khi đóng gói)

| Việc | Trạng thái (v0.4.0) |
|---|---|
| `viewport-fit=cover` + `env(safe-area-inset-*)` cho thanh trên và thanh tab | có |
| Thanh tab **dưới đáy** thay sidebar dưới 900px, mỗi tab ≥ 44px (thanh 64px), ẩn khi bàn phím mở | có |
| Vùng chạm ≥ 44px cho nút, ô chọn, công tắc, chip | có (công tắc 46×26 hiển thị, vùng chạm nới bằng `::before`) |
| Ô nhập `font-size: 16px` trên mobile (iOS không tự zoom) | có |
| Không tràn ngang ở 360×800, 375×812, 390×844 | có (kiểm bằng trình duyệt thật) |
| `100dvh`, cuộn mượt (`overscroll-behavior`, `-webkit-overflow-scrolling`) | có |
| **6 nền dùng chung** với Library (Giấy, Sepia, Xám dịu, Xanh dịu, Đêm, Đen), `theme-color` đổi theo nền | có |
| `manifest.webmanifest` + icon 192/512/maskable + `apple-touch-icon` | có |
| Một lớp `.btn-row` cho mọi hàng nút | có |

Thiếu so với một app native: không có offline, không có thông báo đẩy, không có
chỗ lưu "địa chỉ server" (xem mục 3).

---

## 2. Chọn cách đóng gói

| | **Capacitor 7** | **Tauri 2 mobile** |
|---|---|---|
| Bản chất | WebView của hệ điều hành + plugin native, cấu hình `capacitor.config.ts` | Rust core + WebView hệ điều hành; UI vẫn là web |
| Với app này | **Rất hợp**: SPA React/Vite đã có, chỉ cần `webDir: dist` | Hợp về lâu dài nhưng phải thêm toolchain Rust cho Android/iOS và một lớp IPC |
| Thời gian dựng APK lần đầu | ~1 buổi (cần Android Studio/JDK) | ~1–2 ngày (NDK, Rust targets, cấu hình Gradle/Xcode riêng) |
| Kích thước APK (ước lượng) | 4–7 MB | 6–12 MB |
| Cookie / phiên | WebView chia sẻ cookie như trình duyệt | WebView của hệ điều hành, hành vi cookie khác theo nền tảng |
| Plugin cần cho giai đoạn này | `@capacitor/preferences` để lưu địa chỉ server | phải viết lệnh Rust cho mỗi tính năng hệ thống |
| Rủi ro chính | WebView cũ trên máy Android cũ; gỡ lỗi qua `chrome://inspect` | Build phức tạp, ít tài liệu cho trường hợp "chỉ cần một SPA" |

**Đề xuất: Capacitor**, giống Library. Tauri 2 mobile để dành nếu sau này cần
dùng chung core Rust (đọc SQLite trên máy, đồng bộ nền).

---

## 3. Màn hình "Địa chỉ máy chủ" trước khi đăng nhập

App hiện gọi API **cùng origin** (`fetch('/api/...')` trong `ui/src/main.tsx`,
một hàm `api()` duy nhất). Trong APK, origin là `capacitor://localhost` hoặc
`https://localhost` → mọi lời gọi sẽ trỏ vào WebView chứ không tới NAS. Vì vậy:

1. **Tách API base URL** khỏi `location.origin`:
   - thêm `API_BASE` (mặc định rỗng = cùng origin, giữ nguyên hành vi web);
   - mọi `fetch` đi qua `api()` (đã tập trung một chỗ, chỉ cần đổi chỗ dựng URL).
2. **Màn hình đầu tiên khi chưa có cấu hình** (chỉ trong app đóng gói):
   - ô nhập `https://newsman.lengoc.me` hoặc `http://10.20.2.1:8462`;
   - nút **Kiểm tra** gọi `GET {base}/health` và chỉ lưu khi trả
     `{"ok":true,"service":"lnc-newsman"}`;
   - hiện lỗi rõ ràng (sai tên miền / không tới được / chứng chỉ) chứ không tự đoán.
3. **Lưu an toàn**:
   - Capacitor: `@capacitor/preferences` (Android = SharedPreferences,
     iOS = UserDefaults) cho URL; **không** lưu `MANAGEMENT_KEY` ở đây.
   - Nếu sau này cần nhớ phiên: `@capacitor/preferences` **không** mã hóa — dùng
     keychain/keystore plugin hoặc để cookie WebView tự lo.
4. **Đổi server** phải có trong Cài đặt (xoá cấu hình + về màn hình nhập).

Cùng thiết kế với Library: hai app dùng chung một component/màn hình ở dạng
sao chép file (giống `appLinks.ts` hôm nay), không dựng thư viện chung.

---

## 4. Xác thực cross-origin — chọn cách

Hiện tại: cookie phiên `lnc_newsman_session`, `HttpOnly`, `SameSite=Lax`, `Secure`.
Trong WebView của Capacitor, origin là `capacitor://localhost` → request tới
`https://newsman.lengoc.me` là **cross-origin**, và cookie `SameSite=Lax`
**không** được gửi trong ngữ cảnh này.

| Cách | Việc phải làm | Đánh giá |
|---|---|---|
| **A. Bearer token** (`LNC_API_KEYS` sẵn có, chỉ đọc) | App lưu một API key trong kho bảo mật của máy, gửi `Authorization: Bearer`. Đọc được toàn bộ `/api/v1/*`. | **Đề xuất, khớp Library.** Không đụng CORS/cookie; key chỉ-đọc nên lộ cũng không sửa được gì; thu hồi bằng `LNC_API_KEYS`. Nhược: không dùng `/api/admin/*`, không ghi nhãn. |
| **B. Cookie cross-site** (`SameSite=None; Secure`) + CORS có `Access-Control-Allow-Credentials` | Nới `SameSite`, thêm CORS động theo origin WebView, và cookie dùng được cho cả WebView | **Không khuyến khích**: mở rộng bề mặt, dễ cấu hình sai, ảnh hưởng cả bản web. |
| **C. Đăng nhập trong WebView rồi giữ cookie** | Không làm gì thêm nếu WebView coi origin là chính tên miền | Chỉ ổn định nếu app tải **trực tiếp** `https://newsman.lengoc.me` (không bundle UI). Khi đó mất màn hình nhập server — trái yêu cầu. |

**Chốt**: APK v1 = **A** (bearer, chỉ đọc) + màn hình nhập server. Quản trị giữ
trên web/PWA. Muốn APK làm cả quản trị thì phải chọn B và chấp nhận nới
`SameSite=None` + CORS theo allowlist — một quyết định chung cho cả hai app.

---

## 5. HTTP LAN / cleartext trên Android

- Android 9+ **chặn cleartext** mặc định. `http://10.20.2.1:8462` sẽ không mở được
  cho tới khi khai báo:
  - `android/app/src/main/res/xml/network_security_config.xml`: cho phép
    cleartext **chỉ** cho `10.20.2.1` (và dải `10.20.0.0/16` nếu cần), phần còn lại
    vẫn cấm;
  - `AndroidManifest.xml` → `android:networkSecurityConfig="@xml/network_security_config"`.
- **Khuyến nghị**: mặc định dùng `https://newsman.lengoc.me` (đã có TLS); chỉ bật
  cleartext cho địa chỉ LAN khi người dùng chủ động nhập IP.
- iOS: App Transport Security chặn HTTP tương tự — nếu muốn IP LAN phải khai báo
  `NSExceptionDomains`; PWA không cần vì chạy trong Safari.

---

## 6. iOS

- Build iOS **bắt buộc macOS + Xcode**, cần Apple Developer ID (99 USD/năm) nếu
  muốn cài ngoài TestFlight.
- **Đề xuất**: iOS dùng **PWA** trước — đã có manifest + `apple-touch-icon`,
  "Thêm vào Màn hình chính" cho cảm giác gần native. Khi nào có máy Mac mới làm
  Capacitor iOS.

---

## 7. Các bước khi được duyệt (chưa làm)

1. `ui/`: thêm `API_BASE` + màn hình server + test đơn vị cho `apiUrl()`; giữ
   hành vi web y nguyên khi `API_BASE` rỗng.
2. Tạo `mobile/` (Capacitor): `npx cap init`, `webDir: ../ui/dist`, thêm
   Android platform, `network_security_config`, `Preferences` plugin.
3. Cấu hình CORS phía Rust **chỉ nếu** chọn cách B (bearer thì không cần).
4. Build debug APK trên máy có Android SDK; cài lên một máy Android thật trong LAN,
   kiểm: nhập server → `/health` → mở tin bằng bearer key → 6 nền → xoay màn hình →
   thanh tab dưới đáy không bị thanh điều hướng hệ thống che.
5. Chỉ sau khi (4) đạt mới bàn tới ký release + phân phối nội bộ.

---

## 8. Rủi ro

| Rủi ro | Ảnh hưởng | Cách giảm |
|---|---|---|
| WebView Android cũ (Android 7–8) thiếu `color-mix`, `dvh` | giao diện vỡ trên máy cũ | đặt sàn Android 9+; kiểm bằng WebView thật |
| Cookie `SameSite` khác nhau giữa WebView | đăng nhập "thành công" nhưng mất phiên | dùng bearer (cách A) cho v1 |
| Cleartext bị chặn | không kết nối được IP LAN | `network_security_config` giới hạn đúng IP |
| API key chỉ-đọc nằm trên máy | lộ key = đọc được dữ liệu | key riêng cho mobile, thu hồi ở `LNC_API_KEYS` |
| Chứng chỉ tự ký / tên miền nội bộ | WebView từ chối TLS | dùng tên miền thật có cert; không tắt kiểm chứng |
| Không có offline | mất mạng = trắng trang | v1 chấp nhận; offline cache là giai đoạn sau |
| Hai app lệch phương án | phải làm hai lần, UI khác nhau | bảng §0 là hợp đồng chung; đổi thì sửa cả hai |

---

## 9. Cần chủ dự án quyết (dùng chung với Library)

1. **Cách xác thực**: bearer chỉ-đọc (A, đề xuất) hay cookie cross-site (B)?
2. **APK có làm quản trị không**, hay chỉ đọc? (ảnh hưởng câu 1)
3. **Có máy Mac/Xcode** cho iOS trong 6 tháng tới không? Nếu không, iOS = PWA.
4. **Phân phối APK**: file APK nội bộ, hay cần Google Play (khi đó phải có chính
   sách riêng tư, ký bằng Play App Signing)?
5. **Sàn Android**: 9+ (API 28) hay thấp hơn?
6. **Có cần chạy ngoài LAN** (NetBird trên điện thoại) không? Nếu có, APK phải giữ
   kết nối NetBird — hiện chưa có kế hoạch cho việc này.

Trả lời một lần là áp cho cả Newsman và Library.
