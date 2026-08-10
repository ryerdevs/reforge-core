# ADR-0001: PostgreSQL como base principal sin TimescaleDB por defecto

## Estado

Aceptado

## Fecha

2026-08-06

## Contexto

El servidor actual utiliza C++ y una capa `libsql` basada en la API de MySQL/MariaDB. Su modelo contiene estado transaccional del juego —cuentas, personajes, objetos, inventario, gremios y comercio— además de tablas históricas y de logs.

El proyecto tendrá dos horizontes:

1. Mantener el servidor C++ compatible con Alpine Linux y Docker.
2. Rehacer estructuralmente el servidor en Rust, con una arquitectura más coherente y una posible unificación de las responsabilidades actuales de `game` y `db`.

Se evaluó TimescaleDB porque parte del modelo contiene eventos y registros con marcas temporales. Sin embargo, no se ha demostrado todavía que el volumen, la retención o las consultas analíticas justifiquen añadir una extensión y una operación específica para series temporales.

## Decisión

PostgreSQL estándar será la base de datos principal del futuro servidor Rust.

TimescaleDB no se instalará ni se convertirá en una dependencia inicial. Se evaluará posteriormente únicamente para tablas de telemetría, métricas, auditoría o eventos históricos si las mediciones reales muestran una necesidad clara de particionado temporal, retención, compresión o analítica de alto volumen.

Las tablas de estado principal del juego permanecerán como tablas relacionales PostgreSQL normales.

La fase de compatibilidad del servidor C++ conservará MySQL/MariaDB hasta que exista una estrategia de migración verificada.

## Alternativas consideradas

### PostgreSQL con TimescaleDB desde el inicio

Rechazada por ahora. Aporta capacidades útiles para series temporales, pero añade una dependencia y restricciones operativas antes de tener evidencia de que sean necesarias.

### MariaDB como destino permanente

No elegida para la reescritura. Es la transición más compatible con el C++ actual, pero mantiene la dependencia conceptual de la API y los patrones de MySQL que queremos superar.

### Base de datos distribuida como CockroachDB o YugabyteDB

No elegida. La distribución multi-nodo, los reintentos transaccionales y la complejidad operativa no están justificados para el despliegue inicial de un servidor de juego autocontenido.

### Base de datos especializada de series temporales

No elegida. Introduciría otra tecnología y otra frontera operativa cuando PostgreSQL puede cubrir inicialmente tanto el estado del juego como los logs de menor volumen.

## Consecuencias

### Positivas

- Menos componentes y menor superficie operativa.
- Esquema futuro orientado a PostgreSQL sin arrastrar limitaciones de MySQL.
- Las transacciones y relaciones del estado del juego permanecen en un modelo relacional claro.
- TimescaleDB puede añadirse más adelante sin convertirlo en una decisión irreversible para todo el sistema.
- La infraestructura Docker puede comenzar con un servidor PostgreSQL estándar.

### Negativas

- PostgreSQL estándar puede no ser suficiente para una plataforma de telemetría de gran volumen.
- Si los logs crecen mucho, habrá que diseñar particionado, retención o una solución especializada.
- La migración desde MySQL/MariaDB seguirá requiriendo adaptar tipos, defaults, `ENUM`, `SET`, enteros `UNSIGNED`, fechas inválidas y consultas específicas.

## Condiciones para reevaluar TimescaleDB

La decisión se revisará solo con mediciones y un caso concreto. Los indicadores serán:

- volumen sostenido de inserciones de eventos;
- tamaño y crecimiento de tablas históricas;
- latencia de consultas por rangos temporales;
- coste de retención, compresión y borrado de datos antiguos;
- presión de índices y mantenimiento sobre PostgreSQL estándar;
- necesidad de agregaciones temporales en tiempo real.

La reevaluación deberá incluir un benchmark reproducible, una prueba de backup/restore y una revisión del impacto en Docker, Alpine y la operación diaria.

## No decidido en este ADR

- La librería o crate Rust para acceder a PostgreSQL.
- El diseño definitivo del esquema Rust.
- La separación final entre estado transaccional, eventos y telemetría.
- El procedimiento exacto de migración de datos desde MySQL 5.6.
- Si los eventos históricos vivirán en el mismo clúster PostgreSQL o en una instancia separada.
