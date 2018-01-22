// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! This module defines the verifier's interface.

use environment::Environment;
use data::{VerificationResult, VerificationTask};

/// A verifier builder is an object that lives entire program's
/// lifetime, has no mutable state, and is responsible for constructing
/// verifier instances. The user of this interface is supposed to create
/// a new verifier for each crate he or she wants to verify. The main
/// motivation for having a builder is to be able to cache the JVM
/// initialization.
pub trait VerifierBuilder {
    /// Construct a new verifier object.
    fn new_verifier(&mut self) -> Box<Verifier>;
}

/// A verifier is an object for verifying a single crate, potentially
/// many times.
pub trait Verifier {
    /// Perform a specific verification task.
    ///
    /// A verifier is allowed to mutate its state and preserve state
    /// between `verify` invocations, for example, to cache translation
    /// results. However, verifier is not allowed to cache results from
    /// queries to the environment via facade `env` because these
    /// results may have changed between `verify` invocations.
    ///
    /// `env` is `mut` to allow caching query results.
    fn verify(&mut self, env: &mut Environment, task: &VerificationTask) -> VerificationResult;

    /// Invalidate all caches.
    ///
    /// TODO: Introduce a method `invalidate` that takes a list of
    /// changes and invalidates only caches affected by these changes.
    fn invalidate_all(&mut self, env: &mut Environment);
}
