# Fructosita

[![CI](https://github.com/TU_USUARIO/fructosita/actions/workflows/ci.yml/badge.svg)](https://github.com/TU_USUARIO/fructosita/actions/workflows/ci.yml)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)
[![UCI](https://img.shields.io/badge/protocol-UCI-informational.svg)](https://official-stockfish.github.io/docs/stockfish-wiki/UCI-%26-Commands.html)

Motor de ajedrez UCI, desarrollado desde cero en Rust — sin clonar ni derivar
código de otros motores. Las técnicas usadas (bitboards, alfa-beta, PVS,
null-move pruning, LMR, formato Polyglot, NNUE más adelante) son de dominio
público y estándar en la comunidad; la implementación es original. La única
excepción explícita es el array de 781 números aleatorios del estándar
Polyglot (`book.rs`): por definición debe ser *el mismo* que usan todas las
herramientas compatibles (python-chess, PolyGlot, SCID, etc.) para que los
archivos `.bin` sean intercambiables — es una tabla de constantes públicas,
no lógica de motor.

**Licencia:** GPL-3.0 (ver [`LICENSE`](LICENSE)) — la misma que usan Stockfish
y la gran mayoría de motores libres de la comunidad CCRL/TCEC. Cualquiera
puede usar, estudiar y modificar Fructosita libremente, con la condición de
que las versiones modificadas que se distribuyan también compartan su
código fuente bajo la misma licencia.

## Estado actual: Fase 1 — Motor clásico + libro de aperturas + SEE

- ✅ Bitboards + generación de movimientos legal completa — validado con perft
- ✅ Hash Zobrist incremental (interno, para TT y repeticiones)
- ✅ Evaluación clásica: material, PST tapered, movilidad, estructura de
  peones, seguridad del rey
- ✅ Búsqueda: negamax + alfa-beta, PVS, null-move pruning, LMR, extensión
  por jaque, quiescence search, tabla de transposición
- ✅ **Static Exchange Evaluation (SEE)**: simula la secuencia completa de
  capturas y recapturas en una casilla (con ataques de rayos-x, captura al
  paso y promoción durante el intercambio), no solo "víctima menos
  atacante" como MVV-LVA. Se usa para podar capturas claramente perdedoras
  en quiescence y para ordenar movimientos con mucha más precisión en toda
  la búsqueda — exactamente el tipo de precisión táctica que más pesa en
  finales largos y ajustados.
- ✅ Profundización iterativa con gestión de tiempo real y `stop` (~2ms)
- ✅ Detección de tablas por repetición y regla de 50 movimientos
- ✅ Libro de aperturas en formato Polyglot (.bin), verificado con python-chess
- ✅ Protocolo UCI: `uci`, `isready`, `position`, `go`, `stop`, `setoption
  name Hash/OwnBook/BookFile value <...>`, `d`, `perft`, `ucinewgame`, `quit`

**Fuerza estimada actual:** aproximadamente **2400-2600 Elo** (subiendo desde
~2300-2500 gracias a SEE). Antonio lo probó dos veces contra Krevete (2331
Elo): una derrota de 96 jugadas y otra de 54 — ambas "de cerca", coherente
con un motor competitivo en ese rango que todavía puede afinarse más.

## Compilar y ejecutar

```bash
cargo build --release
./target/release/fructosita          # modo UCI normal (stdin/stdout)
```

Para usar tu propio libro de aperturas, en la GUI/herramienta de torneos
configura las opciones UCI `BookFile` (ruta al archivo `.bin`) y `OwnBook`
(activado por defecto). También se puede probar a mano:

```
setoption name BookFile value C:\ruta\a\tu_libro.bin
position startpos
go wtime 60000 btime 60000
```

### Modos de depuración por línea de comandos

```bash
./target/release/fructosita perft 6
./target/release/fructosita selfplay 60 200   # motor vs. sí mismo (sin libro), prueba de estabilidad
```

### Tests

```bash
cargo test   # 42 tests: movegen, perft, zobrist, evaluación, TT, búsqueda, libro Polyglot, SEE
```

## Validación de corrección

**Generación de movimientos (perft):** validado exactamente contra los
valores de referencia públicos de la comunidad en varias posiciones y
profundidades, incluyendo Perft(7)=3,195,901,860 desde la posición inicial.

**Hash Zobrist interno:** test dedicado que compara, tras cada movimiento de
una secuencia que toca todos los casos especiales, el hash actualizado
incrementalmente contra el recalculado desde cero.

**Libro de aperturas:** los 781 números aleatorios del estándar Polyglot se
extrajeron **programáticamente** (sin transcripción manual, para eliminar
riesgo de error de dedo) del código fuente de `python-chess`, y se
verificaron contra la especificación pública (chessprogramming.org,
hgm.nubati.net). El hash de la posición inicial se comprobó contra el valor
publicado (`0x463b96181691fc9c`). Además, se generó un libro `.bin` de
prueba con `python-chess` (herramienta independiente, no escrita por mí) que
cubre: varias jugadas candidatas con pesos distintos, una continuación, un
enroque codificado con el formato crudo real de Polyglot (e1h1, no e1g1), y
una captura al paso — y se verificó que mi lector en Rust recupera
exactamente los mismos resultados que `python-chess` reporta sobre ese mismo
archivo.

**SEE (Static Exchange Evaluation):** 8 tests dedicados, cada uno con el
resultado esperado verificado a mano antes de correr el código — incluyendo
los tres casos que suelen romper implementaciones apresuradas de SEE: un
ataque de rayos-x a través de una batería de torres (el resultado solo es
correcto si se recalculan los atacantes en cada paso del intercambio), una
captura al paso (la pieza capturada no está en la casilla destino), y una
captura con promoción (lo que queda "de pie" tras la jugada es la dama
recién coronada, no un peón). También un caso que verifica que el bando que
recaptura sabe detenerse cuando seguir la cadena de capturas le conviene
menos que parar.

**Búsqueda:** tests que verifican mates forzados (comprobando la semántica
real), capturas de piezas indefensas, y que evita capturas que pierden
material neto.

**Estabilidad de extremo a extremo:** `selfplay` corrido por 80+
medio-movimientos sin producir nunca un movimiento ilegal, con rendimiento
estable (~650k-900k nodos/s en posiciones de medio juego complejas) tras
integrar SEE.

**Gestión de tiempo:** `stop` corta una búsqueda en ~2ms; `movetime` se
respeta con margen de seguridad.

## Estructura del proyecto

```
src/
├── main.rs      — punto de entrada (UCI / perft / selfplay por CLI)
├── types.rs     — Color, PieceType, Square, notación algebraica
├── bitboard.rs  — tipo Bitboard, utilidades de bits, tablas de ataque
├── board.rs     — estado del tablero, FEN, make_move, hash Zobrist interno
├── zobrist.rs   — claves aleatorias del hash interno (TT/repeticiones)
├── moves.rs     — struct Move / MoveKind, notación UCI
├── movegen.rs   — generación de movimientos pseudo-legales y legales
├── perft.rs     — perft y perft-divide (validación de movegen)
├── eval.rs      — evaluación clásica (material, PST, movilidad, peones, rey)
├── see.rs       — Static Exchange Evaluation (simulación de intercambios)
├── tt.rs        — tabla de transposición
├── search.rs    — negamax, alfa-beta, PVS, null-move, LMR, quiescence, ID
├── book.rs      — lector de libros de apertura Polyglot (.bin)
└── uci.rs       — bucle del protocolo UCI, hilo de búsqueda, gestión de tiempo
testdata/
└── test_book.bin — libro Polyglot de prueba, generado y verificado con python-chess
```

## Hoja de ruta

**Ahora (una sola laptop, sin GPU — objetivo: exprimir el techo clásico):**

- ✅ SEE (Static Exchange Evaluation) — hecho
- **Magic bitboards** (siguiente): reemplaza el método de rayos actual para
  piezas deslizantes — más velocidad bruta, más profundidad en el mismo
  tiempo. Beneficia a todo lo demás (búsqueda en partidas reales,
  generación de datos de autojuego para Fase 2).
- **Lazy SMP** (búsqueda multi-hilo): aprovecha los núcleos de CPU libres
  de la laptop actual para ganar Elo sin depender de GPU.
- **Fase 2 — Fructosita 2.0 "Hexosa"** (Texel tuning): 100% en CPU, no
  necesita la laptop nueva ni GPU. Probablemente el mayor salto de Elo que
  queda disponible en el "techo clásico" (evaluación hecha a mano + PST +
  búsqueda bien afinada), usando partidas reales — incluyendo las de los
  torneos de Antonio — como parte del dataset. Objetivo realista con esta
  fase completa: **~2600-2700 Elo**.

**Después (con la HP EliteBook 845 G8 — Ryzen PRO serie 5000, 6-8 núcleos,
gráficos integrados, sin GPU dedicada):**

Esa laptop no cambia el plan de NNUE (ninguna EliteBook trae GPU dedicada;
Google Colab sigue siendo la ruta para entrenar la red), pero sí permite
trabajar en paralelo entre dos máquinas: generación de partidas de
autojuego mucho más rápida (el cuello de botella real de la Fase 3), testing
SPRT más robusto con más partidas en menos tiempo, y tuning más veloz.

- **Fase 3 — Fructosita 3.0 "Isomerasa"**: NNUE ligera. Autojuego generado
  en las dos laptops en paralelo, entrenamiento en Google Colab (GPU
  gratuita), validación con OpenBench (SPRT). Objetivo: **~2800-3000 Elo**.
- **Tablas de finales Syzygy**: juego perfecto en finales con pocas piezas;
  encaja bien en esta etapa (más espacio en disco disponible entre dos
  equipos para los archivos de tablas).

## Notas de diseño

- **Piezas deslizantes sin magic bitboards (todavía):** método clásico de
  rayo precalculado + recorte en el primer bloqueador (~25-30M nodos/s).
- **Copy-make en vez de make/unmake:** `Board` es `Copy`; más simple y
  menos propenso a bugs.
- **Legalidad por simulación:** cada movimiento pseudo-legal se aplica y se
  comprueba si el propio rey queda en jaque.
- **PST generadas por fórmula, no copiadas:** ver `eval::build_pst`.
- **Búsqueda en hilo separado:** `stop`/`isready` nunca se bloquean.
- **Dos hashes Zobrist distintos, a propósito:** el interno (`zobrist.rs`,
  claves propias) para TT/repeticiones, y el de Polyglot (`book.rs`, claves
  estandarizadas por la comunidad) para leer libros externos. No se
  comparten porque tienen propósitos distintos: uno es privado del motor,
  el otro necesita ser compatible con archivos de terceros.
- **El libro nunca "inventa" un movimiento:** `decode_polyglot_move` jamás
  construye un `Move` directamente desde los bytes crudos; siempre busca
  coincidencia entre los movimientos ya generados como legales por el
  propio motor. Así, un archivo de libro corrupto o de otra variante como
  mucho no aporta ninguna jugada — nunca puede producir un movimiento
  ilegal.
- **SEE recalcula atacantes en cada paso (soporta rayos-x):** en vez de
  fijar de antemano la lista de piezas atacantes, `see::attackers_to` se
  vuelve a llamar tras cada captura simulada, así una torre detrás de otra
  torre (o de una dama) en la misma columna "aparece" correctamente en
  cuanto la pieza que la tapaba se retira del intercambio.

