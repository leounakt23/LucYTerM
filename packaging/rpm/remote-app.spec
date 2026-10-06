Name:           remote-app
Version:        0.1.0
Release:        1%{?dist}
Summary:        Secure native remote computing client
License:        MIT OR Apache-2.0
URL:            https://github.com/LucYTerM/mbxt
Requires:       dbus-libs
Requires:       systemd-libs

%description
Native SSH, SFTP, Telnet, VNC, RDP, serial, and terminal client.

%prep

%build
	cargo build --release --locked
	packaging/scripts/generate-icons.sh

%install
install -Dpm0755 target/release/remote-app %{buildroot}%{_bindir}/remote-app
install -Dpm0644 assets/remote-app.desktop %{buildroot}%{_datadir}/applications/remote-app.desktop
install -Dpm0644 assets/remote-app.metainfo.xml %{buildroot}%{_datadir}/metainfo/remote-app.metainfo.xml
install -Dpm0644 docs/remote-app.1 %{buildroot}%{_mandir}/man1/remote-app.1
install -Dpm0644 assets/icon.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/remote-app.svg
for size in 48 128 256 512; do install -Dpm0644 target/package-icons/${size}x${size}/remote-app.png %{buildroot}%{_datadir}/icons/hicolor/${size}x${size}/apps/remote-app.png; done

%files
%{_bindir}/remote-app
%{_datadir}/applications/remote-app.desktop
%{_datadir}/metainfo/remote-app.metainfo.xml
%{_datadir}/icons/hicolor/scalable/apps/remote-app.svg
%{_datadir}/icons/hicolor/*/apps/remote-app.png
%{_mandir}/man1/remote-app.1*

%post
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database >/dev/null 2>&1 || :

%changelog
* Thu Jan 01 2026 LucYTerM maintainers <maintainers@example.com> - 0.1.0-1
- Initial packaged release.
