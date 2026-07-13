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
        for pt in [
            PieceType::Knight,
            PieceType::Bishop,
            PieceType::Rook,
            PieceType::Queen,
        ] {
            let count = count_bits(board.pieces[color.index()][pt.index()]) as i32;
            phase += count * PHASE_WEIGHT[pt.index()];
        }
    }
    phase.min(MAX_PHASE)
}

fn material_and_pst(board: &Board, color: Color) -> (i32, i32) {
    let (material, pst) = material_and_pst_components(board, color);
    (material.0 + pst.0, material.1 + pst.1)
}

fn material_and_pst_components(board: &Board, color: Color) -> ((i32, i32), (i32, i32)) {
    let p = pst();
    let mut material_mg = 0;
    let mut material_eg = 0;
    let mut pst_mg = 0;
    let mut pst_eg = 0;
    for pt in ALL_PIECE_TYPES {
        let mut bb = board.pieces[color.index()][pt.index()];
        let value = piece_value(pt);
        while bb != EMPTY {
            let sq = pop_lsb(&mut bb);
            let idx = if color == Color::White {
                sq
            } else {
                mirror(sq)
            };
            material_mg += value;
            material_eg += value;
            pst_mg += p.mg[pt.index()][idx as usize];
            pst_eg += p.eg[pt.index()][idx as usize];
        }
    }
    ((material_mg, material_eg), (pst_mg, pst_eg))
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
    let (structure, passed) = pawn_structure_components(board, color);
    (structure.0 + passed.0, structure.1 + passed.1)
}

fn is_passed_pawn(enemy_pawns: u64, color: Color, sq: Square) -> bool {
    let file = file_of(sq);
    let rank = rank_of(sq);
    let lo_file = file.saturating_sub(1);
    let hi_file = (file + 1).min(7);

    if color == Color::White {
        for r in (rank + 1)..8 {
            for f in lo_file..=hi_file {
                if get_bit(enemy_pawns, make_square(f, r)) {
                    return false;
                }
            }
        }
    } else {
        for r in 0..rank {
            for f in lo_file..=hi_file {
                if get_bit(enemy_pawns, make_square(f, r)) {
                    return false;
                }
            }
        }
    }

    true
}

fn is_protected_passed_pawn(own_pawns: u64, color: Color, sq: Square) -> bool {
    let file = file_of(sq);
    let rank = rank_of(sq);
    let protector_rank = match color {
        Color::White => rank.checked_sub(1),
        Color::Black => {
            if rank < 7 {
                Some(rank + 1)
            } else {
                None
            }
        }
    };

    if let Some(r) = protector_rank {
        if file > 0 && get_bit(own_pawns, make_square(file - 1, r)) {
            return true;
        }
        if file < 7 && get_bit(own_pawns, make_square(file + 1, r)) {
            return true;
        }
    }

    false
}

fn is_connected_passed_pawn(own_pawns: u64, sq: Square) -> bool {
    let file = file_of(sq);
    let rank = rank_of(sq);
    let lo_rank = rank.saturating_sub(1);
    let hi_rank = (rank + 1).min(7);

    if file > 0 {
        for r in lo_rank..=hi_rank {
            if get_bit(own_pawns, make_square(file - 1, r)) {
                return true;
            }
        }
    }
    if file < 7 {
        for r in lo_rank..=hi_rank {
            if get_bit(own_pawns, make_square(file + 1, r)) {
                return true;
            }
        }
    }

    false
}

fn promotion_rank_distance(color: Color, sq: Square) -> i32 {
    match color {
        Color::White => 7 - rank_of(sq) as i32,
        Color::Black => rank_of(sq) as i32,
    }
}

fn promotion_square(color: Color, sq: Square) -> Square {
    let rank = match color {
        Color::White => 7,
        Color::Black => 0,
    };
    make_square(file_of(sq), rank)
}

fn king_distance_to_promotion_square(
    board: &Board,
    king_color: Color,
    pawn_color: Color,
    sq: Square,
) -> i32 {
    let king = board.king_square(king_color);
    let promotion = promotion_square(pawn_color, sq);
    let file_distance = (file_of(king) as i32 - file_of(promotion) as i32).abs();
    let rank_distance = (rank_of(king) as i32 - rank_of(promotion) as i32).abs();
    file_distance.max(rank_distance)
}

fn passed_pawn_bonus(board: &Board, own_pawns: u64, color: Color, sq: Square) -> (i32, i32) {
    let advance = match color {
        Color::White => rank_of(sq) as i32,
        Color::Black => 7 - rank_of(sq) as i32,
    };
    let distance = promotion_rank_distance(color, sq);
    let base = advance * advance * 3;
    let advancement = (6 - distance).max(0) * 4;
    let protected = if is_protected_passed_pawn(own_pawns, color, sq) {
        10 + advancement / 2
    } else {
        0
    };
    let connected = if is_connected_passed_pawn(own_pawns, sq) {
        8 + advancement / 2
    } else {
        0
    };
    let own_king_distance = king_distance_to_promotion_square(board, color, color, sq);
    let enemy_king_distance = king_distance_to_promotion_square(board, color.opposite(), color, sq);
    let king_race = (enemy_king_distance - own_king_distance).clamp(-3, 3) * 4;

    let mg = base / 2 + advancement / 2 + protected / 2 + connected / 2 + king_race / 2;
    let eg = base + advancement + protected + connected + king_race;
    (mg.max(0), eg.max(0))
}

fn pawn_structure_components(board: &Board, color: Color) -> ((i32, i32), (i32, i32)) {
    let own_pawns = board.pieces[color.index()][PieceType::Pawn.index()];
    let enemy_pawns = board.pieces[color.opposite().index()][PieceType::Pawn.index()];
    let mut mg = 0;
    let mut eg = 0;
    let mut passed_mg = 0;
    let mut passed_eg = 0;

    for file in 0u8..8 {
        let file_mask: u64 = FILE_A << file;
        let count_on_file = count_bits(own_pawns & file_mask) as i32;
        if count_on_file > 1 {
            mg -= 10 * (count_on_file - 1);
            eg -= 20 * (count_on_file - 1);
        }
        if count_on_file > 0 {
            let mut adjacent: u64 = 0;
            if file > 0 {
                adjacent |= FILE_A << (file - 1);
            }
            if file < 7 {
                adjacent |= FILE_A << (file + 1);
            }
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
        if is_passed_pawn(enemy_pawns, color, sq) {
            let (bonus_mg, bonus_eg) = passed_pawn_bonus(board, own_pawns, color, sq);
            passed_mg += bonus_mg;
            passed_eg += bonus_eg;
        }
    }

    ((mg, eg), (passed_mg, passed_eg))
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

fn knight_outposts(board: &Board) -> i32 {
    let white = knight_outposts_for_color(board, Color::White);
    let black = knight_outposts_for_color(board, Color::Black);
    relative_to_move(board, white - black)
}

fn knight_outposts_for_color(board: &Board, color: Color) -> i32 {
    let mut score = 0;
    let mut knights = board.pieces[color.index()][PieceType::Knight.index()];
    while knights != EMPTY {
        let sq = pop_lsb(&mut knights);
        score += knight_outpost_bonus(board, color, sq);
    }
    score
}

fn knight_outpost_bonus(board: &Board, color: Color, sq: Square) -> i32 {
    if !is_knight_outpost_square(color, sq) {
        return 0;
    }

    let supported = is_supported_by_own_pawn(board, color, sq);
    if !supported || is_attacked_by_enemy_pawn(board, color, sq) {
        return 0;
    }

    let mut bonus = 12 + outpost_centrality_bonus(sq) + outpost_advancement_bonus(color, sq);
    bonus += 10;
    if !enemy_pawn_can_challenge_square(board, color, sq) {
        bonus += 6;
    }
    bonus
}

fn is_knight_outpost_square(color: Color, sq: Square) -> bool {
    let file = file_of(sq);
    if !(2..=5).contains(&file) {
        return false;
    }

    let relative_rank = relative_rank(color, sq);
    (3..=5).contains(&relative_rank)
}

fn is_supported_by_own_pawn(board: &Board, color: Color, sq: Square) -> bool {
    let pawns = board.pieces[color.index()][PieceType::Pawn.index()];
    let file = file_of(sq);
    let rank = rank_of(sq);
    let support_rank = match color {
        Color::White => rank.checked_sub(1),
        Color::Black => {
            if rank < 7 {
                Some(rank + 1)
            } else {
                None
            }
        }
    };

    if let Some(r) = support_rank {
        if file > 0 && get_bit(pawns, make_square(file - 1, r)) {
            return true;
        }
        if file < 7 && get_bit(pawns, make_square(file + 1, r)) {
            return true;
        }
    }

    false
}

fn is_attacked_by_enemy_pawn(board: &Board, color: Color, sq: Square) -> bool {
    is_supported_by_own_pawn(board, color.opposite(), sq)
}

fn enemy_pawn_can_challenge_square(board: &Board, color: Color, sq: Square) -> bool {
    let enemy_pawns = board.pieces[color.opposite().index()][PieceType::Pawn.index()];
    let file = file_of(sq);
    let rank = rank_of(sq);

    for challenge_file in file.saturating_sub(1)..=(file + 1).min(7) {
        if challenge_file == file {
            continue;
        }
        match color {
            Color::White => {
                if rank < 6 && get_bit(enemy_pawns, make_square(challenge_file, rank + 2)) {
                    return true;
                }
                if rank == 3 && get_bit(enemy_pawns, make_square(challenge_file, 6)) {
                    return true;
                }
            }
            Color::Black => {
                if rank > 1 && get_bit(enemy_pawns, make_square(challenge_file, rank - 2)) {
                    return true;
                }
                if rank == 4 && get_bit(enemy_pawns, make_square(challenge_file, 1)) {
                    return true;
                }
            }
        }
    }

    false
}

fn outpost_centrality_bonus(sq: Square) -> i32 {
    match file_of(sq) {
        3 | 4 => 8,
        2 | 5 => 4,
        _ => 0,
    }
}

fn outpost_advancement_bonus(color: Color, sq: Square) -> i32 {
    match relative_rank(color, sq) {
        5 => 8,
        4 => 5,
        3 => 2,
        _ => 0,
    }
}

fn relative_rank(color: Color, sq: Square) -> u8 {
    match color {
        Color::White => rank_of(sq),
        Color::Black => 7 - rank_of(sq),
    }
}

/// Puntuación relativa a quien tiene el turno (positivo = bueno para el que mueve).
pub fn evaluate(board: &Board) -> i32 {
    breakdown(board).total
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EvalBreakdown {
    pub material: i32,
    pub piece_square: i32,
    pub pawn_structure: i32,
    pub passed_pawns: i32,
    pub knight_outposts: i32,
    pub mobility: i32,
    pub king_safety: i32,
    pub tempo: i32,
    pub total: i32,
}

fn taper(mg: i32, eg: i32, phase: i32) -> i32 {
    (mg * phase + eg * (MAX_PHASE - phase)) / MAX_PHASE
}

fn relative_to_move(board: &Board, white_minus_black: i32) -> i32 {
    if board.side_to_move == Color::White {
        white_minus_black
    } else {
        -white_minus_black
    }
}

/// Desglose de la evaluación relativa a quien tiene el turno.
pub fn breakdown(board: &Board) -> EvalBreakdown {
    let (w_mg, w_eg) = material_and_pst(board, Color::White);
    let (b_mg, b_eg) = material_and_pst(board, Color::Black);
    let (w_material, w_pst) = material_and_pst_components(board, Color::White);
    let (b_material, b_pst) = material_and_pst_components(board, Color::Black);
    let (wm_mg, wm_eg) = mobility(board, Color::White);
    let (bm_mg, bm_eg) = mobility(board, Color::Black);
    let (wp_mg, wp_eg) = pawn_structure(board, Color::White);
    let (bp_mg, bp_eg) = pawn_structure(board, Color::Black);
    let (w_pawns, w_passed) = pawn_structure_components(board, Color::White);
    let (b_pawns, b_passed) = pawn_structure_components(board, Color::Black);
    let wk = king_safety(board, Color::White);
    let bk = king_safety(board, Color::Black);
    let outposts = knight_outposts(board);

    let mg = (w_mg + wm_mg + wp_mg + wk) - (b_mg + bm_mg + bp_mg + bk);
    let eg = (w_eg + wm_eg + wp_eg) - (b_eg + bm_eg + bp_eg);

    let phase = game_phase(board);
    let score = taper(mg, eg, phase);

    // Convertimos primero a la perspectiva de quien mueve...
    let relative = relative_to_move(board, score);

    // ...y SOLO DESPUÉS sumamos el bono de tempo: así queda garantizado que
    // beneficia a quien tiene el turno sin importar su color. Sumarlo antes
    // de la conversión (como se hacía originalmente) lo convertía en una
    // penalización para las negras en vez de un bono — bug real, detectado
    // por el test `evaluation_is_color_symmetric`.
    EvalBreakdown {
        material: relative_to_move(
            board,
            taper(
                w_material.0 - b_material.0,
                w_material.1 - b_material.1,
                phase,
            ),
        ),
        piece_square: relative_to_move(board, taper(w_pst.0 - b_pst.0, w_pst.1 - b_pst.1, phase)),
        pawn_structure: relative_to_move(
            board,
            taper(w_pawns.0 - b_pawns.0, w_pawns.1 - b_pawns.1, phase),
        ),
        passed_pawns: relative_to_move(
            board,
            taper(w_passed.0 - b_passed.0, w_passed.1 - b_passed.1, phase),
        ),
        knight_outposts: outposts,
        mobility: relative_to_move(board, taper(wm_mg - bm_mg, wm_eg - bm_eg, phase)),
        king_safety: relative_to_move(board, wk - bk),
        tempo: 10,
        total: relative + outposts + 10,
    }
}

pub fn trace(board: &Board) -> String {
    let side_to_move = match board.side_to_move {
        Color::White => "White",
        Color::Black => "Black",
    };
    let breakdown = breakdown(board);
    format!(
        "eval side_to_move {side_to_move}\n\
         eval material {}\n\
         eval piece_square {}\n\
         eval pawn_structure {}\n\
         eval passed_pawns {}\n\
         eval knight_outposts {}\n\
         eval mobility {}\n\
         eval king_safety {}\n\
         eval tempo {}\n\
         eval total {}\n",
        breakdown.material,
        breakdown.piece_square,
        breakdown.pawn_structure,
        breakdown.passed_pawns,
        breakdown.knight_outposts,
        breakdown.mobility,
        breakdown.king_safety,
        breakdown.tempo,
        breakdown.total
    )
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
        assert!(
            score.abs() < 50,
            "eval de posición inicial fuera de rango: {score}"
        );
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

    #[test]
    fn protected_passed_pawn_scores_higher_than_unprotected() {
        let unprotected = Board::from_fen("4k3/8/8/3P4/8/8/2P5/4K3 w - - 0 1").unwrap();
        let protected = Board::from_fen("4k3/8/8/3P4/2P5/8/8/4K3 w - - 0 1").unwrap();
        assert!(
            breakdown(&protected).passed_pawns > breakdown(&unprotected).passed_pawns,
            "protected passer should increase passed_pawns term"
        );
    }

    #[test]
    fn connected_passed_pawns_score_higher_than_single_passer() {
        let single = Board::from_fen("4k3/8/8/3P4/8/8/8/4K3 w - - 0 1").unwrap();
        let connected = Board::from_fen("4k3/8/8/3PP3/8/8/8/4K3 w - - 0 1").unwrap();
        assert!(
            breakdown(&connected).passed_pawns > breakdown(&single).passed_pawns,
            "connected passers should increase passed_pawns term"
        );
    }

    #[test]
    fn king_supports_passed_pawn_more_than_enemy_king_blockades() {
        let supported = Board::from_fen("4k3/8/3P4/3K4/8/8/8/8 w - - 0 1").unwrap();
        let blockaded = Board::from_fen("3k4/8/3P4/8/8/8/8/4K3 w - - 0 1").unwrap();
        assert!(
            breakdown(&supported).passed_pawns > breakdown(&blockaded).passed_pawns,
            "own king support should outscore enemy king blockade in passed_pawns"
        );
    }

    #[test]
    fn breakdown_total_matches_evaluate_after_passed_pawn_enrichment() {
        let board = Board::from_fen("4k3/8/3PP3/8/8/8/8/3K4 w - - 0 1").unwrap();
        assert_eq!(breakdown(&board).total, evaluate(&board));
    }

    #[test]
    fn breakdown_total_matches_evaluate_after_knight_outposts() {
        let board = Board::from_fen("4k3/8/8/3N4/2P5/8/8/4K3 w - - 0 1").unwrap();
        assert_eq!(breakdown(&board).total, evaluate(&board));
    }

    #[test]
    fn trace_reports_enriched_passed_pawns() {
        let board = Board::from_fen("4k3/8/3PP3/8/8/8/8/3K4 w - - 0 1").unwrap();
        let passed = breakdown(&board).passed_pawns;
        let trace = trace(&board);
        assert!(passed > 0);
        assert!(
            trace.contains(&format!("eval passed_pawns {passed}")),
            "trace should report enriched passed_pawns term: {trace}"
        );
        assert!(
            trace.contains(&format!("eval total {}", breakdown(&board).total)),
            "trace should report total consistently: {trace}"
        );
    }

    #[test]
    fn trace_reports_knight_outposts() {
        let board = Board::from_fen("4k3/8/8/3N4/2P5/8/8/4K3 w - - 0 1").unwrap();
        let outposts = breakdown(&board).knight_outposts;
        let trace = trace(&board);
        assert!(outposts > 0);
        assert!(
            trace.contains(&format!("eval knight_outposts {outposts}")),
            "trace should report knight_outposts term: {trace}"
        );
    }

    #[test]
    fn supported_knight_outpost_scores_above_unsupported_knight() {
        let unsupported = Board::from_fen("4k3/8/8/3N4/8/8/8/4K3 w - - 0 1").unwrap();
        let supported = Board::from_fen("4k3/8/8/3N4/2P5/8/8/4K3 w - - 0 1").unwrap();
        assert!(
            breakdown(&supported).knight_outposts > breakdown(&unsupported).knight_outposts,
            "supported knight outpost should outscore unsupported knight"
        );
    }

    #[test]
    fn enemy_pawn_challenge_reduces_or_removes_outpost_bonus() {
        let secure = Board::from_fen("4k3/8/8/3N4/2P5/8/8/4K3 w - - 0 1").unwrap();
        let challenged = Board::from_fen("4k3/2p5/8/3N4/2P5/8/8/4K3 w - - 0 1").unwrap();
        assert!(
            breakdown(&challenged).knight_outposts < breakdown(&secure).knight_outposts,
            "enemy pawn challenge should reduce knight outpost bonus"
        );
    }

    #[test]
    fn central_outpost_scores_above_rim_knight() {
        let rim = Board::from_fen("4k3/8/8/8/N7/1P6/8/4K3 w - - 0 1").unwrap();
        let central = Board::from_fen("4k3/8/8/3N4/2P5/8/8/4K3 w - - 0 1").unwrap();
        assert!(
            breakdown(&central).knight_outposts > breakdown(&rim).knight_outposts,
            "central outpost should outscore rim knight"
        );
    }

    #[test]
    fn breakdown_total_matches_evaluate_startpos() {
        let board = Board::start_pos();
        assert_eq!(breakdown(&board).total, evaluate(&board));
    }

    #[test]
    fn breakdown_total_matches_evaluate_tactical_position() {
        let board =
            Board::from_fen("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1")
                .unwrap();
        assert_eq!(breakdown(&board).total, evaluate(&board));
    }

    #[test]
    fn trace_contains_total_and_terms() {
        let trace = trace(&Board::start_pos());
        for term in [
            "eval side_to_move",
            "eval material",
            "eval piece_square",
            "eval pawn_structure",
            "eval passed_pawns",
            "eval knight_outposts",
            "eval mobility",
            "eval king_safety",
            "eval tempo",
            "eval total",
        ] {
            assert!(trace.contains(term), "trace missing {term}: {trace}");
        }
    }

    #[test]
    fn evaluation_trace_is_deterministic() {
        let board =
            Board::from_fen("r1bqk2r/2pp1ppp/p1n2n2/1pb1p3/4P3/1B3N2/PPPP1PPP/RNBQ1RK1 w kq - 0 8")
                .unwrap();
        assert_eq!(trace(&board), trace(&board));
    }

    /// Convierte un FEN en su "espejo": tablero volteado verticalmente,
    /// colores de cada pieza intercambiados, turno intercambiado, derechos
    /// de enroque intercambiados y columna de captura al paso reflejada.
    /// Herramienta solo para tests, deliberadamente independiente del
    /// código de `Board`/`eval` (opera como texto sobre el FEN) para no
    /// compartir ningún supuesto con el código que está validando.
    fn mirror_fen(fen: &str) -> String {
        let parts: Vec<&str> = fen.split_whitespace().collect();
        let ranks: Vec<&str> = parts[0].split('/').collect();
        let swap_case = |c: char| {
            if c.is_uppercase() {
                c.to_ascii_lowercase()
            } else if c.is_lowercase() {
                c.to_ascii_uppercase()
            } else {
                c
            }
        };
        let placement: Vec<String> = ranks
            .iter()
            .rev()
            .map(|r| r.chars().map(swap_case).collect())
            .collect();
        let turn = if parts[1] == "w" { "b" } else { "w" };
        let castling: String = if parts[2] == "-" {
            "-".to_string()
        } else {
            parts[2].chars().map(swap_case).collect()
        };
        let ep = if parts[3] == "-" {
            "-".to_string()
        } else {
            let mut chars = parts[3].chars();
            let file = chars.next().unwrap();
            let rank = chars.next().unwrap().to_digit(10).unwrap();
            format!("{file}{}", 9 - rank)
        };
        format!(
            "{} {} {} {} {} {}",
            placement.join("/"),
            turn,
            castling,
            ep,
            parts.get(4).unwrap_or(&"0"),
            parts.get(5).unwrap_or(&"1")
        )
    }

    #[test]
    fn evaluation_is_color_symmetric() {
        // Para varias posiciones reales (con piezas dispersas, enroque
        // disponible, y captura al paso disponible), la evaluación de la
        // posición original y la de su espejo deben coincidir EXACTAMENTE.
        // Esto ejercita material, PST, movilidad, estructura de peones y
        // seguridad del rey a la vez, y probaría cualquier sesgo oculto
        // hacia un color en cualquiera de esos componentes — no solo en la
        // posición inicial (que es simétrica por construcción y no probaría
        // nada), sino en posiciones asimétricas reales.
        let positions = [
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "r1bqk2r/2pp1ppp/p1n2n2/1pb1p3/4P3/1B3N2/PPPP1PPP/RNBQ1RK1 w kq - 0 8",
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
            "rnbqkbnr/ppp1p1pp/8/3pPp2/8/8/PPPP1PPP/RNBQKBNR w KQkq f6 0 4",
        ];
        for fen in positions {
            let board = Board::from_fen(fen).unwrap();
            let mirrored = Board::from_fen(&mirror_fen(fen)).unwrap();
            assert_eq!(
                evaluate(&board),
                evaluate(&mirrored),
                "evaluación no simétrica entre colores para: {fen}"
            );
        }
    }

    #[test]
    fn black_white_symmetry_for_knight_outposts() {
        let fen = "4k3/8/8/3N4/2P5/8/8/4K3 w - - 0 1";
        let board = Board::from_fen(fen).unwrap();
        let mirrored = Board::from_fen(&mirror_fen(fen)).unwrap();
        assert_eq!(
            breakdown(&board).knight_outposts,
            breakdown(&mirrored).knight_outposts,
            "knight outpost evaluation should be color-symmetric"
        );
    }
}
