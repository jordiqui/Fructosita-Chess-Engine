//! Evaluación clásica hecha a mano (sin red neuronal).
//!
//! Devuelve una puntuación en centipeones **relativa a quien tiene el
//! turno** (convención estándar para negamax: positivo = bueno para quien
//! mueve). Combina:
//!   - Material + tablas posicionales (PST) con "tapered eval" (interpola
//!     entre valores de medio juego y de final según cuántas piezas quedan)
//!   - Movilidad (nº de casillas atacadas)
//!   - Estructura de peones (doblados, aislados, pasados)
//!   - Seguridad del rey (columnas abiertas cerca del rey)
//!
//! Las PST no están copiadas de ningún motor existente: se generan por
//! fórmula (distancia al centro, avance de fila, etc.) en `build_pst`, lo
//! cual además las hace fáciles de razonar y ajustar más adelante con Texel
//! tuning (Fase 2).

use crate::bitboard::{count_bits, get_bit, pop_lsb, tables, EMPTY};
use crate::board::Board;
use crate::types::*;
use std::sync::OnceLock;

const FILE_A: u64 = 0x0101010101010101;

pub fn piece_value(p: PieceType) -> i32 {
    match p {
        PieceType::Pawn => 100,
        PieceType::Knight => 320,
        PieceType::Bishop => 330,
        PieceType::Rook => 500,
        PieceType::Queen => 900,
        PieceType::King => 0,
    }
}

const PHASE_WEIGHT: [i32; 6] = [0, 1, 1, 2, 4, 0]; // pawn,knight,bishop,rook,queen,king
const MAX_PHASE: i32 = 24; // 2 bandos * (2N+2B+2R+1Q) = 2*(2+2+4+4) = 24

#[inline(always)]
fn mirror(sq: Square) -> Square {
    sq ^ 56
}

struct Pst {
    mg: [[i32; 64]; 6],
    eg: [[i32; 64]; 6],
}

fn build_pst() -> Pst {
    let mut mg = [[0i32; 64]; 6];
    let mut eg = [[0i32; 64]; 6];

    for sq in 0u8..64 {
        let file = file_of(sq) as f32;
        let rank = rank_of(sq) as f32;
        let cdist = ((file - 3.5).powi(2) + (rank - 3.5).powi(2)).sqrt();
        let i = sq as usize;

        // Peón: favorece columnas centrales y avance; bonus extra por
        // ocupar el centro clásico (d4/e4/d5/e5).
        let file_center = 3.5 - (file - 3.5).abs();
        let mut p_mg = (file_center * 4.0 + rank * 5.0) as i32;
        if (file == 3.0 || file == 4.0) && (rank == 3.0 || rank == 4.0) {
            p_mg += 15;
        }
        mg[PieceType::Pawn.index()][i] = p_mg;
        eg[PieceType::Pawn.index()][i] = (rank * rank * 2.0) as i32;

        // Caballo: "en la banda, se pasma" — penaliza fuerte la distancia al centro.
        mg[PieceType::Knight.index()][i] = (24.0 - cdist * 7.0) as i32;
        eg[PieceType::Knight.index()][i] = (18.0 - cdist * 5.0) as i32;

        // Alfil: centralización más suave que el caballo.
        mg[PieceType::Bishop.index()][i] = (12.0 - cdist * 3.0) as i32;
        eg[PieceType::Bishop.index()][i] = (10.0 - cdist * 3.0) as i32;

        // Torre: preferencia central leve; en el final, bonus por 7ª fila.
        mg[PieceType::Rook.index()][i] = (6.0 - (file - 3.5).abs() * 1.5) as i32;
        eg[PieceType::Rook.index()][i] =
            (4.0 - (file - 3.5).abs()) as i32 + if rank as u8 == 6 { 16 } else { 0 };

        // Dama: centralización suave.
        mg[PieceType::Queen.index()][i] = (6.0 - cdist * 1.5) as i32;
        eg[PieceType::Queen.index()][i] = (10.0 - cdist * 2.5) as i32;

        // Rey: en medio juego prefiere el fondo/esquina (seguridad); en el
        // final, se centraliza (pieza activa).
        mg[PieceType::King.index()][i] = (cdist * 9.0) as i32 + ((7.0 - rank) * 3.0) as i32;
        eg[PieceType::King.index()][i] = (24.0 - cdist * 8.0) as i32;
    }

    Pst { mg, eg }
}

static PST: OnceLock<Pst> = OnceLock::new();
fn pst() -> &'static Pst {
    PST.get_or_init(build_pst)
}

fn game_phase(board: &Board) -> i32 {
    let mut phase = 0;
    for color in [Color::White, Color::Black] {
        for pt in [PieceType::Knight, PieceType::Bishop, PieceType::Rook, PieceType::Queen] {
            let count = count_bits(board.pieces[color.index()][pt.index()]) as i32;
            phase += count * PHASE_WEIGHT[pt.index()];
        }
    }
    phase.min(MAX_PHASE)
}

fn material_and_pst(board: &Board, color: Color) -> (i32, i32) {
    let p = pst();
    let mut mg = 0;
    let mut eg = 0;
    for pt in ALL_PIECE_TYPES {
        let mut bb = board.pieces[color.index()][pt.index()];
        let value = piece_value(pt);
        while bb != EMPTY {
            let sq = pop_lsb(&mut bb);
            let idx = if color == Color::White { sq } else { mirror(sq) };
            mg += value + p.mg[pt.index()][idx as usize];
            eg += value + p.eg[pt.index()][idx as usize];
        }
    }
    (mg, eg)
}

fn mobility(board: &Board, color: Color) -> (i32, i32) {
    let t = tables();
    let occ = board.occupancy();
    let own = board.color_occupancy(color);
    let mut count = 0i32;

    let mut bb = board.pieces[color.index()][PieceType::Knight.index()];
    while bb != EMPTY {
        let sq = pop_lsb(&mut bb);
        count += count_bits(t.knight_attacks(sq) & !own) as i32;
    }
    let mut bb = board.pieces[color.index()][PieceType::Bishop.index()];
    while bb != EMPTY {
        let sq = pop_lsb(&mut bb);
        count += count_bits(t.bishop_attacks(sq, occ) & !own) as i32;
    }
    let mut bb = board.pieces[color.index()][PieceType::Rook.index()];
    while bb != EMPTY {
        let sq = pop_lsb(&mut bb);
        count += count_bits(t.rook_attacks(sq, occ) & !own) as i32;
    }
    let mut bb = board.pieces[color.index()][PieceType::Queen.index()];
    while bb != EMPTY {
        let sq = pop_lsb(&mut bb);
        count += count_bits(t.queen_attacks(sq, occ) & !own) as i32;
    }

    (count * 4, count * 3)
}

fn pawn_structure(board: &Board, color: Color) -> (i32, i32) {
    let own_pawns = board.pieces[color.index()][PieceType::Pawn.index()];
    let enemy_pawns = board.pieces[color.opposite().index()][PieceType::Pawn.index()];
    let mut mg = 0;
    let mut eg = 0;

    for file in 0u8..8 {
        let file_mask: u64 = FILE_A << file;
        let count_on_file = count_bits(own_pawns & file_mask) as i32;
        if count_on_file > 1 {
            mg -= 10 * (count_on_file - 1);
            eg -= 20 * (count_on_file - 1);
        }
        if count_on_file > 0 {
            let mut adjacent: u64 = 0;
            if file > 0 { adjacent |= FILE_A << (file - 1); }
            if file < 7 { adjacent |= FILE_A << (file + 1); }
            if own_pawns & adjacent == EMPTY {
                mg -= 12;
                eg -= 16;
            }
        }
    }

    // Peones pasados: bonus creciente según lo avanzados que estén.
    let mut bb = own_pawns;
    while bb != EMPTY {
        let sq = pop_lsb(&mut bb);
        let file = file_of(sq);
        let rank = rank_of(sq);
        let lo_file = file.saturating_sub(1);
        let hi_file = (file + 1).min(7);

        let mut blocked = false;
        if color == Color::White {
            for r in (rank + 1)..8 {
                for f in lo_file..=hi_file {
                    if get_bit(enemy_pawns, make_square(f, r)) {
                        blocked = true;
                    }
                }
            }
        } else {
            for r in 0..rank {
                for f in lo_file..=hi_file {
                    if get_bit(enemy_pawns, make_square(f, r)) {
                        blocked = true;
                    }
                }
            }
        }
        if !blocked {
            let advance = if color == Color::White { rank } else { 7 - rank };
            let bonus = (advance as i32) * (advance as i32) * 3;
            mg += bonus / 2;
            eg += bonus;
        }
    }

    (mg, eg)
}

/// Heurística ligera: penaliza columnas abiertas/semi-abiertas junto al rey
/// cuando este todavía está cerca de su casa (aprox. "sigue enrocado o sin
/// desarrollar"). No pretende ser un modelo completo de seguridad del rey;
/// eso se refina en Fase 2 con datos reales.
fn king_safety(board: &Board, color: Color) -> i32 {
    let king_sq = board.king_square(color);
    let file = file_of(king_sq);
    let own_pawns = board.pieces[color.index()][PieceType::Pawn.index()];

    let near_home = match color {
        Color::White => rank_of(king_sq) <= 1,
        Color::Black => rank_of(king_sq) >= 6,
    };
    if !near_home {
        return 0;
    }

    let mut score = 0;
    let lo = file.saturating_sub(1);
    let hi = (file + 1).min(7);
    for f in lo..=hi {
        let file_mask: u64 = FILE_A << f;
        if own_pawns & file_mask == EMPTY {
            score -= 15;
        }
    }
    score
}

/// Puntuación relativa a quien tiene el turno (positivo = bueno para el que mueve).
pub fn evaluate(board: &Board) -> i32 {
    let (w_mg, w_eg) = material_and_pst(board, Color::White);
    let (b_mg, b_eg) = material_and_pst(board, Color::Black);
    let (wm_mg, wm_eg) = mobility(board, Color::White);
    let (bm_mg, bm_eg) = mobility(board, Color::Black);
    let (wp_mg, wp_eg) = pawn_structure(board, Color::White);
    let (bp_mg, bp_eg) = pawn_structure(board, Color::Black);
    let wk = king_safety(board, Color::White);
    let bk = king_safety(board, Color::Black);

    let mg = (w_mg + wm_mg + wp_mg + wk) - (b_mg + bm_mg + bp_mg + bk);
    let eg = (w_eg + wm_eg + wp_eg) - (b_eg + bm_eg + bp_eg);

    let phase = game_phase(board);
    let score = (mg * phase + eg * (MAX_PHASE - phase)) / MAX_PHASE;

    // Pequeño bono de "tempo": tener el turno vale algo por sí mismo.
    let score = score + 10;

    if board.side_to_move == Color::White {
        score
    } else {
        -score
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startpos_is_roughly_balanced() {
        let b = Board::start_pos();
        let score = evaluate(&b);
        // Simétrica salvo el bono de tempo: debe estar cerca de 0, nunca
        // desbalanceada como si faltara una pieza (~esto sería cientos de cp).
        assert!(score.abs() < 50, "eval de posición inicial fuera de rango: {score}");
    }

    #[test]
    fn missing_queen_is_heavily_penalized() {
        let with_queen = Board::start_pos();
        let without_queen =
            Board::from_fen("rnb1kbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap();
        // A las negras les falta la dama: para las blancas (quien mueve) debe evaluarse muy a favor.
        assert!(evaluate(&without_queen) > evaluate(&with_queen) + 700);
    }

    #[test]
    fn passed_pawn_close_to_promotion_is_valuable() {
        let far = Board::from_fen("4k3/8/8/8/8/8/P7/4K3 w - - 0 1").unwrap();
        let close = Board::from_fen("4k3/P7/8/8/8/8/8/4K3 w - - 0 1").unwrap();
        assert!(evaluate(&close) > evaluate(&far));
    }
}
