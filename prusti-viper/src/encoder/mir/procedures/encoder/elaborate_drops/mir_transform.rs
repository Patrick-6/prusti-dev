// This file was taken from the compiler:
// https://github.com/rust-lang/rust/blob/661e8beec1fa5f3c58bf6e4362ae3c3fe0b4b1bd/compiler/rustc_mir_transform/src/elaborate_drops.rs
// This file is licensed under Apache 2.0
// (https://github.com/rust-lang/rust/blob/949b98cab8a186b98bf87e64374b8d0848c55271/LICENSE-APACHE)
// and MIT
// (https://github.com/rust-lang/rust/blob/949b98cab8a186b98bf87e64374b8d0848c55271/LICENSE-MIT).

// Changes:
// 1. Fix compilation errors.
// 2. Pull `run_pass` out of `MirPass` (main reason for copying).
// 3. Use our version of MirPatch.

use super::mir_dataflow::{elaborate_drop, DropElaborator};
use log::debug;
use prusti_interface::environment::mir_body::patch::MirPatch;
use prusti_rustc_interface::{
    data_structures::fx::FxHashMap,
    dataflow::{
        elaborate_drops::{DropFlagMode, DropFlagState, DropStyle, Unwind},
        impls::{MaybeInitializedPlaces, MaybeUninitializedPlaces},
        move_paths::{LookupResult, MoveData, MovePathIndex},
        on_all_children_bits, on_all_drop_children_bits, on_lookup_result_bits,
        un_derefer::UnDerefer,
        Analysis, MoveDataParamEnv, ResultsCursor,
    },
    index::bit_set::BitSet,
    middle::{
        mir::*,
        ty::{self, TyCtxt},
    },
    span::{hygiene::DesugaringKind, Span},
    target::abi::VariantIdx,
};
use std::fmt;

/// During MIR building, Drop and DropAndReplace terminators are inserted in every place where a drop may occur.
/// However, in this phase, the presence of these terminators does not guarantee that a destructor will run,
/// as the target of the drop may be uninitialized.
/// In general, the compiler cannot determine at compile time whether a destructor will run or not.
///
/// At a high level, this pass refines Drop and DropAndReplace to only run the destructor if the
/// target is initialized. The way this is achievied is by inserting drop flags for every variable
/// that may be dropped, and then using those flags to determine whether a destructor should run.
/// This pass also removes DropAndReplace, replacing it with a Drop paired with an assign statement.
/// Once this is complete, Drop terminators in the MIR correspond to a call to the "drop glue" or
/// "drop shim" for the type of the dropped place.
///
/// This pass relies on dropped places having an associated move path, which is then used to determine
/// the initialization status of the place and its descendants.
/// It's worth noting that a MIR containing a Drop without an associated move path is probably ill formed,
/// as it would allow running a destructor on a place behind a reference:
///
/// ```text
// fn drop_term<T>(t: &mut T) {
//     mir!(
//         {
//             Drop(*t, exit)
//         }
//         exit = {
//             Return()
//         }
//     )
// }
/// ```
pub(in super::super) fn run_pass<'tcx>(tcx: TyCtxt<'tcx>, body: &mut Body<'tcx>) -> MirPatch<'tcx> {
    debug!("elaborate_drops({:?} @ {:?})", body.source, body.span);

    let def_id = body.source.def_id();
    let param_env = tcx.param_env_reveal_all_normalized(def_id);
    let (side_table, move_data) = match MoveData::gather_moves(body, tcx, param_env) {
        Ok(move_data) => move_data,
        Err((move_data, _)) => {
            tcx.sess.delay_span_bug(
                body.span,
                "No `move_errors` should be allowed in MIR borrowck",
            );
            (Default::default(), move_data)
        }
    };
    let un_derefer = UnDerefer {
        tcx,
        derefer_sidetable: side_table,
    };
    let elaborate_patch = {
        let env = MoveDataParamEnv {
            move_data,
            param_env,
        };
        remove_dead_unwinds(tcx, body, &env, &un_derefer);

        let inits = MaybeInitializedPlaces::new(tcx, body, &env)
            .into_engine(tcx, body)
            .pass_name("elaborate_drops")
            .iterate_to_fixpoint()
            .into_results_cursor(body);

        let uninits = MaybeUninitializedPlaces::new(tcx, body, &env)
            .mark_inactive_variants_as_uninit()
            .into_engine(tcx, body)
            .pass_name("elaborate_drops")
            .iterate_to_fixpoint()
            .into_results_cursor(body);

        let reachable = traversal::reachable_as_bitset(body);

        ElaborateDropsCtxt {
            tcx,
            body,
            env: &env,
            init_data: InitializationData { inits, uninits },
            drop_flags: Default::default(),
            patch: MirPatch::new(body),
            un_derefer,
            reachable,
        }
        .elaborate()
    };
    elaborate_patch //.apply(body);
                    // deref_finder(tcx, body);
}

/// Removes unwind edges which are known to be unreachable, because they are in `drop` terminators
/// that can't drop anything.
pub(in super::super) fn remove_dead_unwinds<'tcx>(
    tcx: TyCtxt<'tcx>,
    body: &mut Body<'tcx>,
    env: &MoveDataParamEnv<'tcx>,
    und: &UnDerefer<'tcx>,
) {
    debug!("remove_dead_unwinds({:?})", body.span);
    // We only need to do this pass once, because unwind edges can only
    // reach cleanup blocks, which can't have unwind edges themselves.
    let mut dead_unwinds = Vec::new();
    let mut flow_inits = MaybeInitializedPlaces::new(tcx, body, env)
        .into_engine(tcx, body)
        .pass_name("remove_dead_unwinds")
        .iterate_to_fixpoint()
        .into_results_cursor(body);
    for (bb, bb_data) in body.basic_blocks.iter_enumerated() {
        let place = match bb_data.terminator().kind {
            TerminatorKind::Drop {
                ref place,
                unwind: Some(_),
                ..
            }
            | TerminatorKind::DropAndReplace {
                ref place,
                unwind: Some(_),
                ..
            } => und.derefer(place.as_ref(), body).unwrap_or(*place),
            _ => continue,
        };

        debug!("remove_dead_unwinds @ {:?}: {:?}", bb, bb_data);

        let LookupResult::Exact(path) = env.move_data.rev_lookup.find(place.as_ref()) else {
            debug!("remove_dead_unwinds: has parent; skipping");
            continue;
        };

        flow_inits.seek_before_primary_effect(body.terminator_loc(bb));
        debug!(
            "remove_dead_unwinds @ {:?}: path({:?})={:?}; init_data={:?}",
            bb,
            place,
            path,
            flow_inits.get()
        );

        let mut maybe_live = false;
        on_all_drop_children_bits(tcx, body, env, path, |child| {
            maybe_live |= flow_inits.contains(child);
        });

        debug!("remove_dead_unwinds @ {:?}: maybe_live={}", bb, maybe_live);
        if !maybe_live {
            dead_unwinds.push(bb);
        }
    }

    if dead_unwinds.is_empty() {
        return;
    }

    let basic_blocks = body.basic_blocks.as_mut();
    for &bb in dead_unwinds.iter() {
        if let Some(unwind) = basic_blocks[bb].terminator_mut().unwind_mut() {
            *unwind = None;
        }
    }
}

struct InitializationData<'mir, 'tcx> {
    inits: ResultsCursor<'mir, 'tcx, MaybeInitializedPlaces<'mir, 'tcx>>,
    uninits: ResultsCursor<'mir, 'tcx, MaybeUninitializedPlaces<'mir, 'tcx>>,
}

impl InitializationData<'_, '_> {
    fn seek_before(&mut self, loc: Location) {
        self.inits.seek_before_primary_effect(loc);
        self.uninits.seek_before_primary_effect(loc);
    }

    fn maybe_live_dead(&self, path: MovePathIndex) -> (bool, bool) {
        (self.inits.contains(path), self.uninits.contains(path))
    }
}

struct Elaborator<'a, 'b, 'tcx> {
    ctxt: &'a mut ElaborateDropsCtxt<'b, 'tcx>,
}

impl fmt::Debug for Elaborator<'_, '_, '_> {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Ok(())
    }
}

impl<'a, 'tcx> DropElaborator<'a, 'tcx> for Elaborator<'a, '_, 'tcx> {
    type Path = MovePathIndex;

    fn patch(&mut self) -> &mut MirPatch<'tcx> {
        &mut self.ctxt.patch
    }

    fn body(&self) -> &'a Body<'tcx> {
        self.ctxt.body
    }

    fn tcx(&self) -> TyCtxt<'tcx> {
        self.ctxt.tcx
    }

    fn param_env(&self) -> ty::ParamEnv<'tcx> {
        self.ctxt.param_env()
    }

    fn drop_style(&self, path: Self::Path, mode: DropFlagMode) -> DropStyle {
        let ((maybe_live, maybe_dead), multipart) = match mode {
            DropFlagMode::Shallow => (self.ctxt.init_data.maybe_live_dead(path), false),
            DropFlagMode::Deep => {
                let mut some_live = false;
                let mut some_dead = false;
                let mut children_count = 0;
                on_all_drop_children_bits(self.tcx(), self.body(), self.ctxt.env, path, |child| {
                    let (live, dead) = self.ctxt.init_data.maybe_live_dead(child);
                    debug!("elaborate_drop: state({:?}) = {:?}", child, (live, dead));
                    some_live |= live;
                    some_dead |= dead;
                    children_count += 1;
                });
                ((some_live, some_dead), children_count != 1)
            }
        };
        match (maybe_live, maybe_dead, multipart) {
            (false, _, _) => DropStyle::Dead,
            (true, false, _) => DropStyle::Static,
            (true, true, false) => DropStyle::Conditional,
            (true, true, true) => DropStyle::Open,
        }
    }

    fn clear_drop_flag(&mut self, loc: Location, path: Self::Path, mode: DropFlagMode) {
        match mode {
            DropFlagMode::Shallow => {
                self.ctxt.set_drop_flag(loc, path, DropFlagState::Absent);
            }
            DropFlagMode::Deep => {
                on_all_children_bits(
                    self.tcx(),
                    self.body(),
                    self.ctxt.move_data(),
                    path,
                    |child| self.ctxt.set_drop_flag(loc, child, DropFlagState::Absent),
                );
            }
        }
    }

    fn field_subpath(&self, path: Self::Path, field: Field) -> Option<Self::Path> {
        prusti_rustc_interface::dataflow::move_path_children_matching(
            self.ctxt.move_data(),
            path,
            |e| match e {
                ProjectionElem::Field(idx, _) => idx == field,
                _ => false,
            },
        )
    }

    fn array_subpath(&self, path: Self::Path, index: u64, size: u64) -> Option<Self::Path> {
        prusti_rustc_interface::dataflow::move_path_children_matching(
            self.ctxt.move_data(),
            path,
            |e| match e {
                ProjectionElem::ConstantIndex {
                    offset,
                    min_length,
                    from_end,
                } => {
                    debug_assert!(size == min_length, "min_length should be exact for arrays");
                    assert!(
                        !from_end,
                        "from_end should not be used for array element ConstantIndex"
                    );
                    offset == index
                }
                _ => false,
            },
        )
    }

    fn deref_subpath(&self, path: Self::Path) -> Option<Self::Path> {
        prusti_rustc_interface::dataflow::move_path_children_matching(
            self.ctxt.move_data(),
            path,
            |e| e == ProjectionElem::Deref,
        )
    }

    fn downcast_subpath(&self, path: Self::Path, variant: VariantIdx) -> Option<Self::Path> {
        prusti_rustc_interface::dataflow::move_path_children_matching(
            self.ctxt.move_data(),
            path,
            |e| match e {
                ProjectionElem::Downcast(_, idx) => idx == variant,
                _ => false,
            },
        )
    }

    fn get_drop_flag(&mut self, path: Self::Path) -> Option<Operand<'tcx>> {
        self.ctxt.drop_flag(path).map(Operand::Copy)
    }
}

struct ElaborateDropsCtxt<'a, 'tcx> {
    tcx: TyCtxt<'tcx>,
    body: &'a Body<'tcx>,
    env: &'a MoveDataParamEnv<'tcx>,
    init_data: InitializationData<'a, 'tcx>,
    drop_flags: FxHashMap<MovePathIndex, Local>,
    patch: MirPatch<'tcx>,
    un_derefer: UnDerefer<'tcx>,
    reachable: BitSet<BasicBlock>,
}

impl<'b, 'tcx> ElaborateDropsCtxt<'b, 'tcx> {
    fn move_data(&self) -> &'b MoveData<'tcx> {
        &self.env.move_data
    }

    fn param_env(&self) -> ty::ParamEnv<'tcx> {
        self.env.param_env
    }

    fn create_drop_flag(&mut self, index: MovePathIndex, span: Span) {
        let tcx = self.tcx;
        let patch = &mut self.patch;
        debug!("create_drop_flag({:?})", self.body.span);
        self.drop_flags
            .entry(index)
            .or_insert_with(|| patch.new_internal(tcx.types.bool, span));
    }

    fn drop_flag(&mut self, index: MovePathIndex) -> Option<Place<'tcx>> {
        self.drop_flags.get(&index).map(|t| Place::from(*t))
    }

    /// create a patch that elaborates all drops in the input
    /// MIR.
    fn elaborate(mut self) -> MirPatch<'tcx> {
        self.collect_drop_flags();

        self.elaborate_drops();

        self.drop_flags_on_init();
        self.drop_flags_for_fn_rets();
        self.drop_flags_for_args();
        self.drop_flags_for_locs();

        self.patch
    }

    fn collect_drop_flags(&mut self) {
        for (bb, data) in self.body.basic_blocks.iter_enumerated() {
            if !self.reachable.contains(bb) {
                continue;
            }
            let terminator = data.terminator();
            let place = match terminator.kind {
                TerminatorKind::Drop { ref place, .. }
                | TerminatorKind::DropAndReplace { ref place, .. } => self
                    .un_derefer
                    .derefer(place.as_ref(), self.body)
                    .unwrap_or(*place),
                _ => continue,
            };

            self.init_data.seek_before(self.body.terminator_loc(bb));

            let path = self.move_data().rev_lookup.find(place.as_ref());
            debug!(
                "collect_drop_flags: {:?}, place {:?} ({:?})",
                bb, place, path
            );

            let path = match path {
                LookupResult::Exact(e) => e,
                LookupResult::Parent(None) => continue,
                LookupResult::Parent(Some(parent)) => {
                    let (_maybe_live, maybe_dead) = self.init_data.maybe_live_dead(parent);

                    if self.body.local_decls[place.local].is_deref_temp() {
                        continue;
                    }

                    if maybe_dead {
                        self.tcx.sess.delay_span_bug(
                            terminator.source_info.span,
                            &format!(
                                "drop of untracked, uninitialized value {:?}, place {:?} ({:?})",
                                bb, place, path
                            ),
                        );
                    }
                    continue;
                }
            };

            on_all_drop_children_bits(self.tcx, self.body, self.env, path, |child| {
                let (maybe_live, maybe_dead) = self.init_data.maybe_live_dead(child);
                debug!(
                    "collect_drop_flags: collecting {:?} from {:?}@{:?} - {:?}",
                    child,
                    place,
                    path,
                    (maybe_live, maybe_dead)
                );
                if maybe_live && maybe_dead {
                    self.create_drop_flag(child, terminator.source_info.span)
                }
            });
        }
    }

    fn elaborate_drops(&mut self) {
        for (bb, data) in self.body.basic_blocks.iter_enumerated() {
            if !self.reachable.contains(bb) {
                continue;
            }
            let loc = Location {
                block: bb,
                statement_index: data.statements.len(),
            };
            let terminator = data.terminator();

            let resume_block = self.patch.resume_block();
            match terminator.kind {
                TerminatorKind::Drop {
                    mut place,
                    target,
                    unwind,
                } => {
                    if let Some(new_place) = self.un_derefer.derefer(place.as_ref(), self.body) {
                        place = new_place;
                    }

                    self.init_data.seek_before(loc);
                    match self.move_data().rev_lookup.find(place.as_ref()) {
                        LookupResult::Exact(path) => elaborate_drop(
                            &mut Elaborator { ctxt: self },
                            terminator.source_info,
                            place,
                            path,
                            target,
                            if data.is_cleanup {
                                Unwind::InCleanup
                            } else {
                                Unwind::To(Option::unwrap_or(unwind, resume_block))
                            },
                            bb,
                        ),
                        LookupResult::Parent(..) => {
                            if !matches!(
                                terminator.source_info.span.desugaring_kind(),
                                Some(DesugaringKind::Replace),
                            ) {
                                self.tcx.sess.delay_span_bug(
                                    terminator.source_info.span,
                                    &format!("drop of untracked value {:?}", bb),
                                );
                            }
                            // A drop and replace behind a pointer/array/whatever.
                            // The borrow checker requires that these locations are initialized before the assignment,
                            // so we just leave an unconditional drop.
                            assert!(!data.is_cleanup);
                        }
                    }
                }
                TerminatorKind::DropAndReplace {
                    mut place,
                    ref value,
                    target,
                    unwind,
                } => {
                    assert!(!data.is_cleanup);

                    if let Some(new_place) = self.un_derefer.derefer(place.as_ref(), self.body) {
                        place = new_place;
                    }
                    self.elaborate_replace(loc, place, value, target, unwind);
                }
                _ => continue,
            }
        }
    }

    /// Elaborate a MIR `replace` terminator. This instruction
    /// is not directly handled by codegen, and therefore
    /// must be desugared.
    ///
    /// The desugaring drops the location if needed, and then writes
    /// the value (including setting the drop flag) over it in *both* arms.
    ///
    /// The `replace` terminator can also be called on places that
    /// are not tracked by elaboration (for example,
    /// `replace x[i] <- tmp0`). The borrow checker requires that
    /// these locations are initialized before the assignment,
    /// so we just generate an unconditional drop.
    fn elaborate_replace(
        &mut self,
        loc: Location,
        place: Place<'tcx>,
        value: &Operand<'tcx>,
        target: BasicBlock,
        unwind: Option<BasicBlock>,
    ) {
        let bb = loc.block;
        let data = &self.body[bb];
        let terminator = data.terminator();
        assert!(
            !data.is_cleanup,
            "DropAndReplace in unwind path not supported"
        );

        let assign = Statement {
            kind: StatementKind::Assign(Box::new((place, Rvalue::Use(value.clone())))),
            source_info: terminator.source_info,
        };

        let unwind = unwind.unwrap_or_else(|| self.patch.resume_block());
        let unwind = self.patch.new_block(BasicBlockData {
            statements: vec![assign.clone()],
            terminator: Some(Terminator {
                kind: TerminatorKind::Goto { target: unwind },
                ..*terminator
            }),
            is_cleanup: true,
        });

        let target = self.patch.new_block(BasicBlockData {
            statements: vec![assign],
            terminator: Some(Terminator {
                kind: TerminatorKind::Goto { target },
                ..*terminator
            }),
            is_cleanup: false,
        });

        match self.move_data().rev_lookup.find(place.as_ref()) {
            LookupResult::Exact(path) => {
                debug!(
                    "elaborate_drop_and_replace({:?}) - tracked {:?}",
                    terminator, path
                );
                self.init_data.seek_before(loc);
                elaborate_drop(
                    &mut Elaborator { ctxt: self },
                    terminator.source_info,
                    place,
                    path,
                    target,
                    Unwind::To(unwind),
                    bb,
                );
                on_all_children_bits(self.tcx, self.body, self.move_data(), path, |child| {
                    self.set_drop_flag(
                        Location {
                            block: target,
                            statement_index: 0,
                        },
                        child,
                        DropFlagState::Present,
                    );
                    self.set_drop_flag(
                        Location {
                            block: unwind,
                            statement_index: 0,
                        },
                        child,
                        DropFlagState::Present,
                    );
                });
            }
            LookupResult::Parent(parent) => {
                // drop and replace behind a pointer/array/whatever. The location
                // must be initialized.
                debug!(
                    "elaborate_drop_and_replace({:?}) - untracked {:?}",
                    terminator, parent
                );
                self.patch.patch_terminator(
                    bb,
                    TerminatorKind::Drop {
                        place,
                        target,
                        unwind: Some(unwind),
                    },
                );
            }
        }
    }

    fn constant_bool(&self, span: Span, val: bool) -> Rvalue<'tcx> {
        Rvalue::Use(Operand::Constant(Box::new(Constant {
            span,
            user_ty: None,
            literal: ConstantKind::from_bool(self.tcx, val),
        })))
    }

    fn set_drop_flag(&mut self, loc: Location, path: MovePathIndex, val: DropFlagState) {
        if let Some(&flag) = self.drop_flags.get(&path) {
            let span = self.patch.source_info_for_location(self.body, loc).span;
            let val = self.constant_bool(span, val.value());
            self.patch.add_assign(loc, Place::from(flag), val);
        }
    }

    fn drop_flags_on_init(&mut self) {
        let loc = Location::START;
        let span = self.patch.source_info_for_location(self.body, loc).span;
        let false_ = self.constant_bool(span, false);
        for flag in self.drop_flags.values() {
            self.patch
                .add_assign(loc, Place::from(*flag), false_.clone());
        }
    }

    fn drop_flags_for_fn_rets(&mut self) {
        for (bb, data) in self.body.basic_blocks.iter_enumerated() {
            if !self.reachable.contains(bb) {
                continue;
            }
            if let TerminatorKind::Call {
                destination,
                target: Some(tgt),
                cleanup: Some(_),
                ..
            } = data.terminator().kind
            {
                assert!(!self.patch.is_patched(bb));

                let loc = Location {
                    block: tgt,
                    statement_index: 0,
                };
                let path = self.move_data().rev_lookup.find(destination.as_ref());
                on_lookup_result_bits(self.tcx, self.body, self.move_data(), path, |child| {
                    self.set_drop_flag(loc, child, DropFlagState::Present)
                });
            }
        }
    }

    fn drop_flags_for_args(&mut self) {
        let loc = Location::START;
        prusti_rustc_interface::dataflow::drop_flag_effects_for_function_entry(
            self.tcx,
            self.body,
            self.env,
            |path, ds| {
                self.set_drop_flag(loc, path, ds);
            },
        )
    }

    fn drop_flags_for_locs(&mut self) {
        // We intentionally iterate only over the *old* basic blocks.
        //
        // Basic blocks created by drop elaboration update their
        // drop flags by themselves, to avoid the drop flags being
        // clobbered before they are read.

        for (bb, data) in self.body.basic_blocks.iter_enumerated() {
            if !self.reachable.contains(bb) {
                continue;
            }
            debug!("drop_flags_for_locs({:?})", data);
            for i in 0..(data.statements.len() + 1) {
                debug!("drop_flag_for_locs: stmt {}", i);
                let mut allow_initializations = true;
                if i == data.statements.len() {
                    match data.terminator().kind {
                        TerminatorKind::Drop { .. } => {
                            // drop elaboration should handle that by itself
                            continue;
                        }
                        TerminatorKind::DropAndReplace { .. } => {
                            // this contains the move of the source and
                            // the initialization of the destination. We
                            // only want the former - the latter is handled
                            // by the elaboration code and must be done
                            // *after* the destination is dropped.
                            assert!(self.patch.is_patched(bb));
                            allow_initializations = false;
                        }
                        TerminatorKind::Resume => {
                            // It is possible for `Resume` to be patched
                            // (in particular it can be patched to be replaced with
                            // a Goto; see `MirPatch::new`).
                        }
                        _ => {
                            assert!(!self.patch.is_patched(bb));
                        }
                    }
                }
                let loc = Location {
                    block: bb,
                    statement_index: i,
                };
                prusti_rustc_interface::dataflow::drop_flag_effects_for_location(
                    self.tcx,
                    self.body,
                    self.env,
                    loc,
                    |path, ds| {
                        if ds == DropFlagState::Absent || allow_initializations {
                            self.set_drop_flag(loc, path, ds)
                        }
                    },
                )
            }

            // There may be a critical edge after this call,
            // so mark the return as initialized *before* the
            // call.
            if let TerminatorKind::Call {
                destination,
                target: Some(_),
                cleanup: None,
                ..
            } = data.terminator().kind
            {
                assert!(!self.patch.is_patched(bb));

                let loc = Location {
                    block: bb,
                    statement_index: data.statements.len(),
                };
                let path = self.move_data().rev_lookup.find(destination.as_ref());
                on_lookup_result_bits(self.tcx, self.body, self.move_data(), path, |child| {
                    self.set_drop_flag(loc, child, DropFlagState::Present)
                });
            }
        }
    }
}
