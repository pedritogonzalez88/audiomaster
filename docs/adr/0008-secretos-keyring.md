# ADR-008 — Secretos con el keyring del sistema operativo

- **Estado:** Aceptado
- **Fecha:** 2026-11-15
- **Decisores:** Arquitectura, Seguridad

## Contexto

La app maneja credenciales de streaming (Icecast/Shoutcast), API keys de
proveedores de tandas y, si se habilita, el token del logger. No hay servidor
central donde guardar secretos, y escribirlos en `app_settings` (SQLite en
texto plano) los expone a cualquiera con acceso al perfil del usuario.

## Decisión

- Los secretos se guardan en el **keyring del SO**:
  - Windows: Credential Manager
  - macOS: Keychain
  - Linux: Secret Service (GNOME Keyring / KWallet)
- Se usa el crate `keyring` (o `tauri-plugin-stronghold` si se necesita
  portabilidad a WebView) con un *namespace* por estación:
  `service = "com.audiomaster.studio"`, `user = "station:<id>:<key>"`.
- En SQLite solo se guarda una **referencia** (`app_settings.value_json` con
  `{"keyring":"station:1:stream.password"}`), nunca el valor.
- Los secretos se leen al iniciar sesión y se mantienen en memoria el menor
  tiempo posible (`secrecy::SecretString`); nunca se serializan en logs ni en
  DTO IPC.
- Si el keyring no está disponible (Linux sin servicio), la app arranca en modo
  degradado: pide la contraseña en cada sesión y avisa en la barra superior.
- La contraseña de usuario de la app usa **argon2id** (`password_hash`,
  `password_algo`), no el keyring, porque debe verificarse en el servidor de
  escritura única.

## Consecuencias

- Positivas: sin secretos en backups de SQLite; integración con SSO del SO.
- Negativas: comportamiento distinto por plataforma; hay que testear el modo
  degradado en CI (Ubuntu headless no tiene Secret Service).
- Riesgo: el keyring no protege contra un proceso con los mismos privilegios del
  usuario; se documenta en el threat model.

## Alternativas consideradas

| Alternativa | Motivo del rechazo |
| --- | --- |
| Texto plano en SQLite | Viola el requisito de no exponer credenciales. |
| Archivo cifrado con clave derivada | Requiere una contraseña maestra extra para el operador. |
| Variables de entorno | No persisten entre sesiones de escritorio. |
