# Guía de Compilación y Pruebas de PeakGuard

**Objetivo:** Compilación nativa en Windows 10/11 x64 sin dependencias de Visual Studio ni elevación de privilegios.

---

## 1. Prerrequisitos

1. **Rust Toolchain (GNU Target):**
   ```powershell
   rustup target add x86_64-pc-windows-gnu
   rustup default stable-x86_64-pc-windows-gnu
   ```

2. **MinGW-w64 GCC Linker (instalable mediante Scoop):**
   ```powershell
   scoop install mingw
   ```
   Asegúrese de que `gcc.exe` esté en su `PATH`:
   `C:\Users\<Usuario>\scoop\apps\mingw\current\bin`

---

## 2. Compilación del Núcleo

Desde la carpeta `core/`:

### Compilación en Modo Debug (Rápida para desarrollo)
```powershell
cargo build
```
El ejecutable resultante se ubica en:
`core\target\x86_64-pc-windows-gnu\debug\peakguard.exe`

### Compilación en Modo Release (Optimizado, ~6 MB binario autónomo)
```powershell
cargo build --release
```
El ejecutable optimizado se ubica en:
`core\target\x86_64-pc-windows-gnu\release\peakguard.exe`

---

## 3. Ejecución de Pruebas Automatizadas

Para correr la suite de pruebas unitarias y de validación de configuración:
```powershell
cargo test
```

---

## 4. Comandos de Verificación del Binario

```powershell
# 1. Verificar versión y arquitectura
.\core\target\x86_64-pc-windows-gnu\release\peakguard.exe version

# 2. Validar configuración TOML
.\core\target\x86_64-pc-windows-gnu\release\peakguard.exe config validate "config\peakguard.example.toml"

# 3. Ejecutar diagnóstico de entorno y puertos
.\core\target\x86_64-pc-windows-gnu\release\peakguard.exe doctor
```
