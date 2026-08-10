# Cliente: implementación del modo desarrollo con assets sueltos

> **Metadata**
> - Type: History
> - Status: Historical
> - Audience: Project agents and maintainers (historical context only)
> - Last verified: 2026-08-10
> - Original location: `docs/superpowers/plans/2026-08-06-client-assets-dev-mode-implementation.md`
> - **Historical record.** Archived for context. This document is NOT current normative guidance: it was the 2026-08-06 plan for a loose-assets development mode against the old repo layout (`client/Client/Client/...`). The current client tree is `source\client\` and the pack workflow uses `source\tools\pack\PackMakerLite.exe` — do not follow the paths or tasks literally.

> **Para agentes:** ejecuta este plan tarea por tarea. Usa la guía local de C++/CMake, depuración y documentación del proyecto antes de modificar archivos de implementación.

**Meta:** preparar una distribución exclusiva para desarrollo en la que el cliente lea todos sus recursos desde `client/Client/Client/assets`, sin inicializar `pack/Index` ni depender de archivos `.eix` o `.epk`, tal como define el spec `docs/history/2026-08-06-client-assets-design.md`.

**Arquitectura:** mantener `CEterPackManager` como adaptador temporal. El arranque deja de leer `pack/Index` y configura el adaptador en modo archivo con raíz `<directorio-de-ejecución>/assets`. La resolución de rutas virtuales (`pc/`, `locale/`, `uiscript/`, ...) se antepone a la raíz `assets` dentro de `Get`, `isExist` y las operaciones de lectura. La extracción es una operación de preparación de la distribución developer, no una funcionalidad del cliente en tiempo de ejecución.

**Stack:** C++ legacy del cliente (`ClientVS22`, toolset `v143`), Python 2.7 para el script de extracción o alternativa C++, PowerShell para la validación en Windows/WSL.

---

## Alcance y no objetivos

Este plan cubre la extracción reproducible de los packs a archivos sueltos y la activación del modo archivo en el cliente. No incluye hot reload, un módulo de assets en Rust, sustituir `CEterPackManager` por una API nueva, cambios en DirectX, `ResourceManager`, scripts Python ni formatos de texturas, ni compatibilidad de la distribución developer con packs comprimidos.

### Evidencia actual que condiciona el diseño

- `client/Client/Client/pack/Index` tiene 249 líneas: líneas alternadas de directorio virtual y nombre de pack. El marcador `*` al inicio indica la raíz virtual (`patch1`, `season3_eu`, `patch2`, ... se resuelven desde la raíz). Líneas con sufijo `/` indican un directorio virtual; el pack siguiente se mapea bajo ese directorio.
- Los packs base se mapean bajo `d:/ymir work/...` y `pack/` (por ejemplo `d:/ymir work/pc/` -> `pc`, `sound/ambience/` -> `sound`).
- Los packs de patch se registran primero en el `Index` (máxima prioridad), seguidos de los packs base. La resolución de `Get` recorre los packs en el orden en que se registraron.
- El cliente ya soporta el modo `FILE`: `CEterPackManager::SetSearchMode(false)` y `SetRelativePathMode` habilitan la búsqueda de archivos sueltos antes que en packs (confirmado en `client/Game/EterPack/EterPackManager.cpp`).
- La validación debe ejecutarse desde `client/Client/Client`, que es el directorio de trabajo del ejecutable.

## Tarea 1: Inventario del estado actual del cliente

**Archivos:**
- Leer: `client/Client/Client/pack/Index`
- Leer: `client/Game/EterPack/EterPackManager.cpp`
- Leer: `client/Game/EterPack/EterPack.cpp`

- [ ] **Paso 1: Listar packs, tamaños y el índice**

```powershell
Get-ChildItem .\client\Client\Client\pack -Filter *.epk | Sort-Object Name | Select-Object Name,@{N='MB';E={[math]::Round($_.Length/1MB,1)}}
Get-Content .\client\Client\Client\pack\Index
```

Registrar en este plan el número total de packs, el tamaño total y las rutas virtuales de mayor prioridad.

- [ ] **Paso 2: Confirmar el orden de prioridad y el modo FILE**

Leer `EterPackManager::RegisterPack`, `EterPackManager::Get` y `EterPackManager::SetSearchMode` para confirmar: los packs se buscan en orden de registro; `SetSearchMode(false)` habilita la búsqueda de archivos sueltos por encima de los packs. Documentar el comportamiento exacto en la sección de validación.

- [ ] **Paso 3: Documentar los formatos de pack que se van a extraer**

Identificar en `EterPack.cpp` si el pack es de tipo binario (`PACK`/`FILE` con cabecera) o texto. Para cada tipo, documentar: cómo se lee el índice, cómo se localiza cada entrada (offset y tamaño), y qué cifrado/compresión (si existe) aplica a los datos. Esto es insumo del script de extracción.

## Tarea 2: Construir la herramienta de extracción

**Archivos:**
- Crear: `client/Client/Client/tools/extract_assets.py` (Python 2.7, alineado con `python27.dll`) o `client/Client/Client/tools/extract_assets.cpp` si se prefiere C++.

- [ ] **Paso 1: Implementar el lector de packs**

Implementar en el script la lectura de un `.epk` y su `.eix` siguiendo el formato confirmado en la Tarea 1. La salida debe ser un diccionario `{ruta virtual: (offset, tamaño)}`.

- [ ] **Paso 2: Implementar la resolución del `Index`**

Parsear `pack/Index` con el formato alternado (directorio virtual + nombre de pack), respetando:
- `*` al inicio = raíz virtual.
- Líneas con sufijo `/` = directorio virtual para el pack siguiente.
- Líneas sin sufijo = nombre de pack (`.epk`/`.eix`).

Construir una lista ordenada `[(directorio virtual, nombre de pack)]` en el orden del archivo. Esta lista define la prioridad de resolución.

- [ ] **Paso 3: Implementar la extracción con prioridad**

Para cada ruta virtual solicitada, buscar en los packs en orden de prioridad y extraer la primera coincidencia. La extracción debe:
1. crear el directorio destino `client/Client/Client/assets/<ruta virtual>`;
2. escribir el archivo suelto con el contenido del pack (bytes crudos, sin descomprimir ni descifrar);
3. **no sobrescribir** un archivo ya extraído con una versión de menor prioridad (el primero que coincida gana).

- [ ] **Paso 4: Generar el inventario de extracción**

El script debe emitir:
- un manifiesto `assets/manifest.json` con la lista de rutas extraídas y el pack de origen;
- un informe de conflictos: rutas presentes en varios packs (con el pack ganador y los perdedores);
- un informe de packs huérfanos: packs listados en `Index` sin archivos asociados.

- [ ] **Paso 5: Probar la extracción en un subconjunto**

Extraer primero solo `pc/`, `uiscript/` y `locale/en/` (packs de prioridad base) y verificar que los archivos existen en `assets/` con las rutas virtuales esperadas. No extraer todo hasta que el subconjunto valide el script.

## Tarea 3: Extraer todos los assets

**Archivos:**
- Crear: `client/Client/Client/assets/` (resultado)

- [ ] **Paso 1: Ejecutar la extracción completa**

```powershell
python .\client\Client\Client\tools\extract_assets.py
```

Registrar en el plan: número de archivos extraídos, tamaño total en disco, número de conflictos resueltos y packs huérfanos.

- [ ] **Paso 2: Verificar el inventario**

Comparar el `manifest.json` contra el `Index`: toda ruta del `Index` debe tener su archivo en `assets/`. Los archivos faltantes deben listarse como errores. Los packs huérfanos (sin entradas en `Index` pero con `.epk`/`.eix` en `pack/`) se documentan y no se eliminan en esta fase.

## Tarea 4: Activar el modo archivo en el cliente

**Archivos:**
- Modificar: `client/Game/EterPack/EterPackManager.cpp`
- Modificar: `client/Game/EterPack/EterPackManager.h`
- Modificar (si existe): `client/Game/Application.cpp` o el punto de arranque que llama a `PackInitialize`

- [ ] **Paso 1: Configurar la raíz de assets en el arranque**

Modificar `PackInitialize` para que:
1. no abra `pack/Index` ni registre packs;
2. configure el modo archivo: `SetSearchMode(false)` y `SetRelativePathMode(true)` (según la API real);
3. registre la raíz `assets` como directorio raíz del adaptador.

La ruta debe resolverse relativa al directorio de trabajo (`<directorio-de-ejecución>/assets`).

- [ ] **Paso 2: Aplicar la raíz en la resolución**

En `Get`, `isExist` y las operaciones de lectura del adaptador, anteponer la raíz `assets` a la ruta virtual solicitada antes de resolver en el sistema de archivos. Las llamadas externas siguen pasando rutas virtuales.

- [ ] **Paso 3: Fallar explícitamente sin `assets`**

Si no existe `assets` en el arranque, registrar un error explícito con la ruta esperada y abortar la inicialización (sin intentar recuperar desde packs).

- [ ] **Paso 4: Validar que las rutas no escapen de la raíz**

Rechazar rutas que contengan `..` o que resuelvan fuera de `assets`. Registrar un error y devolver fallo para la ruta.

## Tarea 5: Compilar y validar la distribución developer

**Archivos:**
- Modificar: `client/Client/ClientVS22/...` (configuración del proyecto `UserInterface`, toolset `v143`)
- Crear: `client/Client/ClientVS22/build_developer.cmd` o instrucción documentada

- [ ] **Paso 1: Compilar el cliente en configuración developer**

Compilar el proyecto `UserInterface` con el toolset `v143`. El resultado debe ser el ejecutable en `client/Client/Client`.

- [ ] **Paso 2: Arrancar el cliente y verificar carga inicial**

Ejecutar desde `client/Client/Client`:
- el cliente debe arrancar sin errores de `pack/Index`;
- debe cargar los recursos iniciales desde `assets/` (login, UIs, modelos de personaje);
- los logs no deben contener errores de archivo faltante relacionados con `pack/`.

- [ ] **Paso 3: Modificar un recurso y verificar en el siguiente arranque**

Modificar un recurso suelto (por ejemplo un archivo de `uiscript/`) y arrancar de nuevo. Confirmar que el cambio se refleja sin volver a empaquetar nada.

## Tarea 6: Eliminar los packs de la distribución developer

**Archivos:**
- Eliminar (después de la validación): `client/Client/Client/pack/*.eix` y `client/Client/Client/pack/*.epk`
- Conservar: `client/Client/Client/pack/Index` (referencia) o eliminar si la validación lo permite

- [ ] **Paso 1: Verificar referencias restantes**

Buscar referencias en tiempo de ejecución a `pack/Index`, `.eix` y `.epk` en el código del cliente:

```powershell
rg -n "pack/Index|\.eix|\.epk" .\client\Game .\client\Client\ClientVS22 --glob '*.cpp' --glob '*.h'
```

Solo se eliminan los packs si no quedan referencias activas.

- [ ] **Paso 2: Eliminar los packs**

Eliminar `*.eix` y `*.epk` de `client/Client/Client/pack`. No eliminar archivos fuente ni bibliotecas.

- [ ] **Paso 3: Re-validar el arranque sin packs**

Repetir la validación de la Tarea 5 con `pack/` sin `.eix`/`.epk`. Confirmar que el cliente sigue cargando todo desde `assets/`.

## Tarea 7: Documentar y cerrar

**Archivos:**
- Modificar: `docs/history/2026-08-06-client-assets-design.md` (agregar estado: implementado)
- Crear: `docs/operations/client-assets-dev-mode.md` (guía operativa)
- Modificar: `AGENTS.md` con los comandos verificados y limitaciones

- [ ] **Paso 1: Documentar la guía operativa**

Documentar: cómo se ejecuta la extracción, cómo se activa el modo dev, cómo se compila, cómo se valida un cambio de recurso, y qué se elimina de la distribución.

- [ ] **Paso 2: Documentar limitaciones**

Documentar que la distribución developer no soporta packs comprimidos, que el rendimiento de carga puede ser menor con archivos sueltos, y que `assets` no debe commitearse completa si el repo no lo permite (usar `.gitignore` para `assets/` si aplica).

## Lista de verificación de verificación

Antes de declarar el plan completo, ejecutar la skill de verificación del proyecto y confirmar con salida fresca:

- [ ] `extract_assets.py` extrae todo el `Index` sin errores y el manifiesto no tiene faltantes.
- [ ] El cliente compila en configuración developer con toolset `v143`.
- [ ] El ejecutable arranca desde `client/Client/Client` y carga recursos iniciales desde `assets/` sin `pack/Index`.
- [ ] Un cambio en un recurso suelto se usa en el siguiente arranque sin reempaquetar.
- [ ] `pack/` no contiene `.eix` ni `.epk` al final.
- [ ] No quedan referencias activas a `pack/Index`, `.eix` o `.epk` en el código del cliente.
- [ ] La documentación identifica el modo dev y sus limitaciones.

Si la extracción no puede validarse en el entorno activo (por ejemplo, Python 2.7 no disponible), reportar el prerrequisito exacto que falta y detenerse en validación estática; no afirmar que la extracción pasó.
