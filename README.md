# ArtCraft Launcher

Lanzador, descargador y actualizador de las **Crafting Apps** de
[ArtCraft](https://getartcraft.com/apps/) (PhotoCraft, FilmCraft, VectorCraft…)
para Linux. UI nativa en Rust con [egui](https://github.com/emilk/egui).

Las apps se publican como GitHub Releases en la organización
[`storytold`](https://github.com/storytold). Este programa consulta esas
releases, descarga el AppImage de tu arquitectura, verifica su SHA256, lo
instala por usuario y crea los accesos directos para que aparezcan en rofi,
ulauncher y cualquier menú de aplicaciones.

## Características

- **Catálogo automático:** detecta los repos de la organización cuyo nombre
  termina en `craft`, así que las apps nuevas aparecen solas. Las que aún no
  tienen build para Linux salen como "sin versión para Linux todavía".
- **Instalar y actualizar** con barra de progreso. Al actualizar se elimina la
  versión anterior.
- **Verificación de integridad:** compara el SHA256 con el `SHA256SUMS.txt` de
  la release y descarta la descarga si no coincide.
- **Accesos directos:** reutiliza el `.desktop` y los iconos que ya trae cada
  AppImage, reescribiendo `Exec` con la ruta real.
- **Ventana tipo launcher:** flotante y colocada arriba a la derecha (i3/X11).
  `Esc` la cierra, y se cierra sola al abrir una app.
- **Sin sobrecargar la API de GitHub:** caché en disco con ETag (las respuestas
  304 no cuentan para el límite de 60 peticiones/hora).

## Instalación

Requisitos: Rust (`cargo`) y `fuse2` para ejecutar AppImages.

```sh
git clone <este-repo> artcraft-launcher
cd artcraft-launcher
./install.sh
```

`install.sh` compila en release y deja:

| Qué | Dónde |
|---|---|
| Binario | `~/.local/bin/artcraft-launcher` |
| Icono | `~/.local/share/icons/hicolor/scalable/apps/artcraft-launcher.svg` |
| Acceso directo | `~/.local/share/applications/artcraft-launcher.desktop` |

Asegúrate de que `~/.local/bin` está en tu `PATH`.

## Uso

Abre **ArtCraft Launcher** desde rofi/ulauncher, o ejecuta `artcraft-launcher`.
Para abrirlo con un atajo en i3:

```
bindsym $mod+a exec artcraft-launcher
```

### Línea de comandos

| Comando | Qué hace |
|---|---|
| `artcraft-launcher --list` | Lista el catálogo con la versión instalada y la última |
| `artcraft-launcher --install <app>` | Instala o actualiza una app sin abrir la ventana |
| `artcraft-launcher --sync-desktop` | Regenera los accesos directos de lo ya instalado |

### Variables de entorno

- `GITHUB_TOKEN` / `GH_TOKEN`: opcional. Sube el límite de la API de GitHub de
  60 a 5000 peticiones/hora.

## Dónde guarda las cosas

| Qué | Dónde |
|---|---|
| AppImages | `~/.local/share/artcraft-launcher/apps/<app>/` |
| Estado (qué hay instalado) | `~/.local/share/artcraft-launcher/installed.json` |
| Accesos directos | `~/.local/share/applications/artcraft-<app>.desktop` |
| Iconos de las apps | `~/.local/share/icons/hicolor/` |
| Caché de GitHub | `~/.cache/artcraft-launcher/` |

Los `.desktop` creados por el launcher llevan la marca
`X-ArtCraft-Managed=true`; al desinstalar solo se borran los que la tienen.

## Estructura

```
src/main.rs      UI (egui), modo CLI y colocación de la ventana
src/github.rs    cliente de la API de GitHub con caché ETag y descargas
src/store.rs     catálogo, instalación, verificación y estado local
src/desktop.rs   accesos directos e iconos
src/wm.rs        posicionamiento de la ventana vía IPC de i3
assets/          icono del launcher
install.sh       compila e instala para el usuario actual
```

## Limitaciones

- Solo gestiona **AppImage** (`linux-x86_64` / `linux-aarch64`). Una app que
  publique únicamente `.deb`, `.rpm` o `.tar.gz` se mostrará como sin versión
  para Linux.
- No usa las actualizaciones delta (`.zsync`) que publican algunas releases:
  cada actualización descarga el archivo completo.
- La comprobación de versiones es manual (botón "Actualizar lista").
- La colocación arriba a la derecha solo funciona en **i3 sobre X11**. En otros
  entornos la ventana se abre donde decida el gestor de ventanas.
- ArtCraft (la app original) no tiene build para Linux, así que no se puede
  instalar desde aquí.

## Licencia

Pendiente de decidir.
