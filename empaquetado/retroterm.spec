# RPM de RetroTerm para openSUSE, Fedora y EL10 (Rocky / Alma Linux 10).
# Copyright (C) 2026 Alfonso García Tejeo <agtejeo@ugr.es> - GPL-3.0-or-later
#
# Este .spec NO compila: empaqueta un binario ya compilado dentro de un
# contenedor de la distribución de destino (lo hace construir-rpm.sh).
# Así cada RPM queda enlazado con las librerías de SU distribución.
#
# Las dependencias de librerías (GTK 4, VTE, D-Bus...) no se escriben a
# mano: rpmbuild analiza el ejecutable y añade solo los "Requires" de las
# .so que usa (p. ej. libvte-2.91-gtk4.so.0). Cada gestor (zypper, dnf)
# sabe qué paquete de su distribución las proporciona.

# Sin paquete -debuginfo (no compilamos dentro de rpmbuild).
%global debug_package %{nil}
%global app_id es.personal.RetroTerm

Name:           retroterm
# La versión llega desde construir-rpm.sh (la lee de Cargo.toml).
Version:        %{version_rt}
Release:        1%{?dist}
Summary:        Emulador de terminal con estética retro, perfiles SSH y KeePass
License:        GPL-3.0-or-later AND OFL-1.1
Packager:       Alfonso García Tejeo <agtejeo@ugr.es>
Vendor:         Alfonso García Tejeo
Source0:        retroterm
Source1:        %{app_id}.desktop
Source2:        %{app_id}.svg
Source3:        VT323-Regular.ttf
Source4:        OFL-VT323.txt
Source5:        LICENSE
Source6:        CHANGELOG.md
Source7:        retroterm-suse
Source8:        retroterm-redhat

# Dependencia de FICHERO: vale en cualquier distribución, se llame como se
# llame el paquete que trae ssh (openssh-clients, openssh...).
Requires:       /usr/bin/ssh

%description
RetroTerm: emulador de terminal personal con fuente y cursor retro, fósforo
verde/ámbar/blanco, efectos CRT, pestañas, perfiles de conexión SSH,
contraseñas en el llavero del sistema, guiones post-conexión y lectura de
contraseñas de root desde un fichero KeePass.

Autor: Alfonso García Tejeo <agtejeo@ugr.es>.
Licencia: GNU GPL versión 3 o posterior. Incluye la fuente VT323
(SIL Open Font License 1.1).

%prep
# Nada que preparar: no hay código fuente que desempaquetar.

%build
# Nada que compilar: ver la cabecera.

%install
install -Dm0755 %{SOURCE0} %{buildroot}%{_bindir}/retroterm
install -Dm0644 %{SOURCE1} %{buildroot}%{_datadir}/applications/%{app_id}.desktop
install -Dm0644 %{SOURCE2} %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/%{app_id}.svg
install -Dm0644 %{SOURCE3} %{buildroot}%{_datadir}/fonts/retroterm/VT323-Regular.ttf
install -Dm0644 %{SOURCE4} %{buildroot}%{_datadir}/licenses/%{name}/OFL-VT323.txt
install -Dm0644 %{SOURCE5} %{buildroot}%{_datadir}/licenses/%{name}/LICENSE
install -Dm0644 %{SOURCE6} %{buildroot}%{_docdir}/%{name}/CHANGELOG.md
# Servicio PAM para el desbloqueo: cada familia incluye su pila de autenticación.
%if 0%{?suse_version}
install -Dm0644 %{SOURCE7} %{buildroot}%{_sysconfdir}/pam.d/retroterm
%else
install -Dm0644 %{SOURCE8} %{buildroot}%{_sysconfdir}/pam.d/retroterm
%endif

%post
# Que el sistema vea la fuente y el icono nuevos sin reiniciar la sesión.
fc-cache -f %{_datadir}/fonts/retroterm >/dev/null 2>&1 || :
touch --no-create %{_datadir}/icons/hicolor >/dev/null 2>&1 || :

%postun
fc-cache -f >/dev/null 2>&1 || :

%files
%{_bindir}/retroterm
%{_datadir}/applications/%{app_id}.desktop
%{_datadir}/icons/hicolor/scalable/apps/%{app_id}.svg
%dir %{_datadir}/fonts/retroterm
%{_datadir}/fonts/retroterm/VT323-Regular.ttf
%dir %{_datadir}/licenses/%{name}
%{_datadir}/licenses/%{name}/OFL-VT323.txt
%{_datadir}/licenses/%{name}/LICENSE
%dir %{_docdir}/%{name}
%{_docdir}/%{name}/CHANGELOG.md
# noreplace: si el administrador lo edita, una actualización no lo pisa.
%config(noreplace) %{_sysconfdir}/pam.d/retroterm

%changelog
* Wed Nov 18 2026 Alfonso García Tejeo <agtejeo@ugr.es> - %{version_rt}-1
- Ver /usr/share/doc/retroterm/CHANGELOG.md para el historial completo.
