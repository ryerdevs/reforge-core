# Cliente: assets sueltos para desarrollo

> **Metadata**
> - Type: History
> - Status: Historical
> - Audience: Project agents and maintainers (historical context only)
> - Last verified: 2026-08-10
> - Original location: `docs/superpowers/specs/2026-08-06-client-assets-development-mode-design.md`
> - **Historical record.** Archived for context. This document is NOT current normative guidance: it was the 2026-08-06 design for a loose-assets development mode against the old repo layout (`client/Client/Client/...`). The current client tree is `source\client\` and the pack workflow uses `source\tools\pack\PackMakerLite.exe` — the design was not carried into the current repo layout.

## Objetivo

Preparar una distribución exclusiva para desarrollo en la que el cliente lea todos sus recursos desde `client/Client/Client/assets`, sin inicializar `pack/Index` ni depender de archivos `.eix` o `.epk`.

Esta fase prioriza la simplicidad del flujo de trabajo: modificar un archivo dentro de `assets` debe ser suficiente para que el cliente lo use en el siguiente arranque. No incluye hot reload, Rust, un nuevo formato de compresión ni una reestructuración general del sistema de recursos.

## Alcance

Incluye:

- Extraer los recursos de los packs actuales a `client/Client/Client/assets`.
- Mantener las rutas virtuales que el cliente ya solicita (`pc/`, `locale/`, `uiscript/`, etc.).
- Hacer que el arranque del cliente configure una raíz de recursos en `assets`.
- Mantener `CEterPackManager` como interfaz temporal para que `ResourceManager`, Python y las clases de recursos existentes no necesiten una migración amplia.
- Hacer que las búsquedas de archivos sueltos se resuelvan debajo de `assets`.
- Eliminar después de la validación los archivos `.eix` y `.epk` de la distribución developer.

No incluye:

- Hot reload.
- Implementación del módulo de assets en Rust.
- Sustitución de `CEterPackManager` por una API nueva.
- Cambios en DirectX, `ResourceManager`, scripts Python o formatos de texturas.
- Compatibilidad de la distribución developer con packs comprimidos.
- Eliminación de otros archivos de `pack` que no sean `.eix` o `.epk`, salvo que la validación demuestre que son residuos sin uso.

## Enfoques considerados

### A. Cambiar cada consumidor para usar `assets`

Modificar `ResourceManager`, Python, sonido, localización y cada llamada directa a archivos.

Esto ofrece una separación más limpia a largo plazo, pero implica una auditoría amplia y aumenta el riesgo de dejar un consumidor usando una ruta distinta.

### B. Mantener `CEterPackManager` como adaptador de archivos sueltos — elegido

Conservar la interfaz existente y modificar solamente su resolución de archivos para anteponer la raíz `assets`. El arranque deja de leer `pack/Index` y configura el adaptador en modo archivo.

Es el cambio mínimo que permite que la mayoría de consumidores existentes sigan funcionando sin una migración transversal. También deja un límite claro para sustituirlo más adelante por un módulo Rust.

### C. Crear un proveedor nuevo y migrar solo algunos consumidores

Introducir una nueva abstracción de assets y conectar gradualmente algunos sistemas.

Es una buena dirección para la migración a Rust, pero añade interfaces y código que no son necesarios para esta fase de desarrollo.

## Diseño aprobado

### Flujo de arranque

`PackInitialize` dejará de abrir `pack/Index`, registrar packs o registrar `root`. En su lugar, inicializará el modo de archivos de `CEterPackManager` con una raíz configurable fija para esta distribución:

```text
<directorio-de-ejecución>/assets
```

El cliente no intentará buscar recursos en `pack/` y no habrá fallback a `.eix` o `.epk`.

### Resolución de recursos

Las rutas que ya usa el cliente seguirán siendo virtuales. Por ejemplo:

```text
pc/warrior/warrior_m.msm
```

se resolverá como:

```text
assets/pc/warrior/warrior_m.msm
```

La raíz se aplicará dentro de `Get`, `isExist` y las operaciones de lectura del adaptador existente. Las llamadas externas continuarán pasando rutas virtuales y no conocerán detalles del directorio físico.

### Extracción y prioridad

Los packs se extraerán respetando el orden efectivo de `pack/Index`. Cuando varios packs contengan la misma ruta, se conservará como versión final la que actualmente tendría prioridad en el cliente. Antes de eliminar los packs se generará una comprobación de inventario para detectar conflictos de rutas y archivos faltantes.

La extracción es una operación de preparación de la distribución developer, no una funcionalidad del cliente en tiempo de ejecución.

### Archivos eliminados

Después de verificar la extracción y de comprobar que no quedan referencias activas a `pack/Index`, `.eix` o `.epk`, se eliminarán los archivos `.eix` y `.epk` de `client/Client/Client/pack`. No se eliminarán archivos fuente ni bibliotecas del proyecto.

### Errores

- Si no existe `assets`, el arranque debe fallar de forma explícita y registrar la ruta esperada.
- Si falta un recurso solicitado, se conservará el comportamiento de error del consumidor existente.
- No se intentará recuperar el recurso desde un pack comprimido.
- Las rutas de recursos no deben escapar de la raíz `assets` mediante `..`.

### Verificación

La validación de esta fase deberá comprobar:

1. Que `assets` contiene los recursos extraídos y conserva las rutas virtuales esperadas.
2. Que el inventario de archivos necesarios no tiene faltantes después de eliminar los packs.
3. Que no quedan referencias de ejecución a `pack/Index`, `.eix` o `.epk` en el código del cliente.
4. Que el proyecto `UserInterface` compila en la configuración developer con el toolset `v143`.
5. Que el ejecutable arranca desde `client/Client/Client` y puede cargar los recursos iniciales sin depender de `pack`.

## Criterio de aceptación

La fase se considera terminada cuando el cliente developer compila, arranca y carga los recursos desde `assets` con `pack/` sin archivos `.eix` ni `.epk`, y una modificación de un recurso suelto se utiliza en el siguiente arranque sin volver a empaquetar nada.

## Fuera de esta fase

La futura migración a Rust podrá reemplazar este adaptador por un `asset_core`, pero no se anticipará esa arquitectura en el código actual. El siguiente paso independiente será diseñar el plan de implementación y extracción con comandos reproducibles.
