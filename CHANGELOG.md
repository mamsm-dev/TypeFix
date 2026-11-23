# Changelog

Each version's section is used as the GitHub release notes when its `v*` tag is pushed
(the release workflow fails if the section is missing). Newest version first.

## [0.5.3]

### Fixed
- Wayland: the shortcut worked only some of the time. The shortcut's modifier keys are now released before the paste, the selection read is retried, and the delays before pasting and before restoring the clipboard are longer so slower applications get the text.

### فارسی
- Wayland: میان‌بر فقط گاهی کار می‌کرد. اکنون کلیدهای تغییردهندهٔ میان‌بر پیش از چسباندن رها می‌شوند، خواندن انتخاب چند بار تکرار می‌شود و مکث پیش از چسباندن و پیش از برگرداندن کلیپ‌بورد بیشتر است تا برنامه‌های کندتر هم متن را بگیرند.

## [0.5.2]

### Changed
- Wayland: `typefix` and `typefix-gui` now log each step of the shortcut (portal registration, activation, clipboard, paste) to the terminal to help diagnose desktops where the shortcut does nothing. The selected text itself is never logged.

### فارسی
- Wayland: برنامه مراحل میان‌بر (ثبت در پورتال، فعال‌شدن، کلیپ‌بورد، چسباندن) را در ترمینال گزارش می‌کند تا عیب‌یابی ساده‌تر شود. متن انتخاب‌شده هرگز ثبت نمی‌شود.

## [0.5.1]

### Fixed
- Wayland: the shortcut worker crashed at start-up with "there is no reactor running" because the portal library was built for Tokio. It now uses the async-std runtime. Internal errors are shown in the window instead of a generic message.

### فارسی
- Wayland: کارگر میان‌بر هنگام شروع با خطای «there is no reactor running» از کار می‌افتاد، چون کتابخانهٔ پورتال برای Tokio ساخته شده بود. اکنون از async-std استفاده می‌کند و خطاهای داخلی در پنجره نمایش داده می‌شوند.

## [0.5.0]

### Added
- Wayland support for the conversion shortcut (tested target: KDE Plasma). The shortcut is registered through the XDG GlobalShortcuts portal, so you can also change it in System Settings, and the converted text is pasted through the RemoteDesktop portal (keyboard only). The desktop asks for permission the first time.
- The selection is read from PRIMARY and written to the clipboard through the Wayland data-control protocol.
- Typing suggestions remain X11-only.

### Changed
- The desktop file is now `io.github.mamsmdev.TypeFix.desktop` so portals can identify the app.

### فارسی
- پشتیبانی از Wayland برای میان‌بر تبدیل (هدف آزمایش: KDE Plasma). میان‌بر از طریق پورتال GlobalShortcuts ثبت می‌شود و از تنظیمات سیستم هم قابل تغییر است؛ جای‌گذاری متن با پورتال RemoteDesktop (فقط صفحه‌کلید) انجام می‌شود. بار اول سیستم اجازه می‌پرسد.
- پیشنهاد حین تایپ فقط روی X11 کار می‌کند.
- نام فایل desktop به `io.github.mamsmdev.TypeFix.desktop` تغییر کرد.

## [0.4.0] - 2026-11-21

### Added
- System tray icon with the TypeFix icon. Click it, or choose "Open TypeFix", to show the settings window; "Quit" stops TypeFix. Closing the window still keeps TypeFix running in the background.
- Release notes: every release now shows the changes of its version, taken from this file.

### Fixed
- Converting selected text now replaces the selection. Before, the converted text was inserted next to the original text, which stayed in place.

### فارسی
- **جدید:** آیکون TypeFix در سینی سیستم نمایش داده می‌شود. با کلیک روی آن یا گزینهٔ «باز کردن TypeFix» پنجرهٔ تنظیمات باز می‌شود و «خروج» برنامه را متوقف می‌کند. بستن پنجره هنوز برنامه را در پس‌زمینه فعال نگه می‌دارد.
- **جدید:** هر ریلیز اکنون تغییرات همان نسخه را (از همین فایل) نشان می‌دهد.
- **اصلاح:** تبدیل متن انتخاب‌شده حالا خود متن را جایگزین می‌کند. قبلاً متن تبدیل‌شده کنار متن اصلی درج می‌شد و متن اصلی باقی می‌ماند.

## [0.3.0] - 2026-11-21

### Added
- Settings window (`typefix-gui`, GTK 3): record a new conversion shortcut, turn typing suggestions on or off, choose the theme (system, light, dark; accent color #18B0B0) and the interface language (English or Persian, right-to-left).
- New application icon.
- Packages (AppImage, deb, rpm, Arch) include `typefix-gui` and a desktop entry.

### فارسی
- **جدید:** پنجرهٔ تنظیمات (`typefix-gui`، با GTK 3): ضبط میان‌بر تبدیل، روشن/خاموش کردن پیشنهاد حین تایپ، انتخاب تم (سیستم، روشن، تیره؛ رنگ اصلی #18B0B0) و زبان رابط (انگلیسی یا فارسی، راست‌به‌چپ).
- **جدید:** آیکون جدید برنامه.
- **جدید:** بسته‌ها (AppImage، deb، rpm، Arch) حالا `typefix-gui` و فایل desktop را هم دارند.

## [0.2.0] - 2026-11-21

### Added
- Select text in any application and press a shortcut (default Ctrl+Alt+Space) to convert it between the English and Persian keyboard layouts. The shortcut can be changed with `--shortcut` or in `~/.config/typefix/config`.

### Changed
- Typing suggestions are now opt-in (`--suggest`). By default only the shortcut is grabbed and typing is not observed.

### فارسی
- **جدید:** متن را در هر برنامه‌ای انتخاب کنید و با یک میان‌بر (پیش‌فرض Ctrl+Alt+Space) آن را بین چیدمان انگلیسی و فارسی تبدیل کنید. میان‌بر با `--shortcut` یا در `~/.config/typefix/config` قابل تغییر است.
- **تغییر:** پیشنهاد حین تایپ اکنون اختیاری است (`--suggest`). به‌طور پیش‌فرض فقط میان‌بر گرفته می‌شود و تایپ زیر نظر نیست.

## [0.1.1] - 2026-11-21

### Fixed
- Typing detection did not see any key while an application window had focus, so no suggestion was ever shown on a real desktop.

### فارسی
- **اصلاح:** تشخیص تایپ وقتی پنجرهٔ یک برنامه فوکوس داشت هیچ کلیدی نمی‌دید و روی دسکتاپ واقعی هیچ پیشنهادی نمایش داده نمی‌شد.

## [0.1.0] - 2026-11-18

### Added
- First Linux release: `typefix` foreground app for X11 that detects English text typed with the Persian layout (and the reverse) and shows the conversion as a desktop notification.
- Packages: AppImage, deb, rpm and Arch.

### فارسی
- **جدید:** اولین نسخهٔ لینوکس: برنامهٔ foreground با نام `typefix` برای X11 که متن انگلیسیِ تایپ‌شده با چیدمان فارسی (و برعکس) را تشخیص می‌دهد و تبدیل آن را به‌شکل اعلان نشان می‌دهد.
- **جدید:** بسته‌ها: AppImage، deb، rpm و Arch.
