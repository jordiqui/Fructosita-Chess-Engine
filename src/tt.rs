//! Tabla de transposición (TT): caché de posiciones ya analizadas, indexada
//! por hash Zobrist. Evita re-analizar la misma posición cuando se llega a
//! ella por distintos órdenes de movimientos (algo muy frecuente en
//! ajedrez), y guarda el mejor movimiento encontrado para mejorar el
//! ordenamiento de movimientos en visitas futuras.

use crate::moves::Move;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TTFlag {
    /// El valor guardado es exacto (se completó una búsqueda con ventana abierta).
    Exact,
    /// El valor guardado es una cota inferior (hubo poda beta: el valor real es >= score).
    LowerBound,
    /// El valor guardado es una cota superior (ningún movimiento mejoró alfa: el valor real es <= score).
    UpperBound,
}

#[derive(Clone, Copy)]
struct TTEntry {
    key: u64,
    depth: i8,
    score: i32,
    flag: TTFlag,
    best_move: Option<Move>,
}

pub struct TTProbe {
    pub depth: i8,
    pub score: i32,
    pub flag: TTFlag,
    pub best_move: Option<Move>,
}

pub struct TranspositionTable {
    entries: Vec<Option<TTEntry>>,
    mask: usize,
}

impl TranspositionTable {
    pub fn new(mb: usize) -> Self {
        let bytes = mb.max(1) * 1024 * 1024;
        let entry_size = std::mem::size_of::<Option<TTEntry>>().max(1);
        let mut count = (bytes / entry_size).max(1024);
        count = count.next_power_of_two();
        if count > (bytes / entry_size).max(1024) && count > 1024 {
            count /= 2; // no exceder demasiado el tamaño pedido
        }
        count = count.max(1024);
        TranspositionTable { entries: vec![None; count], mask: count - 1 }
    }

    pub fn resize(&mut self, mb: usize) {
        *self = TranspositionTable::new(mb);
    }

    pub fn clear(&mut self) {
        for e in self.entries.iter_mut() {
            *e = None;
        }
    }

    #[inline(always)]
    fn index(&self, key: u64) -> usize {
        (key as usize) & self.mask
    }

    pub fn probe(&self, key: u64) -> Option<TTProbe> {
        let idx = self.index(key);
        if let Some(entry) = &self.entries[idx] {
            if entry.key == key {
                return Some(TTProbe {
                    depth: entry.depth,
                    score: entry.score,
                    flag: entry.flag,
                    best_move: entry.best_move,
                });
            }
        }
        None
    }

    pub fn store(&mut self, key: u64, depth: i32, score: i32, flag: TTFlag, best_move: Option<Move>) {
        let idx = self.index(key);
        let depth = depth.clamp(0, i8::MAX as i32) as i8;
        let replace = match &self.entries[idx] {
            None => true,
            Some(existing) => existing.key == key || existing.depth <= depth,
        };
        if replace {
            self.entries[idx] = Some(TTEntry { key, depth, score, flag, best_move });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_and_probe_roundtrip() {
        let mut tt = TranspositionTable::new(1);
        assert!(tt.probe(12345).is_none());
        tt.store(12345, 5, 100, TTFlag::Exact, None);
        let probe = tt.probe(12345).unwrap();
        assert_eq!(probe.depth, 5);
        assert_eq!(probe.score, 100);
        assert_eq!(probe.flag, TTFlag::Exact);
    }

    #[test]
    fn clear_empties_table() {
        let mut tt = TranspositionTable::new(1);
        tt.store(1, 1, 1, TTFlag::Exact, None);
        tt.clear();
        assert!(tt.probe(1).is_none());
    }
}
