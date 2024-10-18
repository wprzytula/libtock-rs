// #![cfg_attr(not(test), no_std)]
#![no_std]
#![warn(unsafe_op_in_unsafe_fn)]

use core::marker::PhantomData;

use libtock_platform::Syscalls;

pub trait TockFuture<S: Syscalls>: Sized {
    type Output;
    fn check_resolved(&self) -> bool;
    fn await_completion(self) -> Self::Output;

    fn select<Other: TockFuture<S>>(self, other: Other) -> Select<S, Self, Other> {
        Select {
            fut1: self,
            fut2: other,
            s: PhantomData,
        }
    }

    fn join<Other: TockFuture<S>>(self, other: Other) -> Join<S, Self, Other> {
        Join {
            fut1: self,
            fut2: other,
            s: PhantomData,
        }
    }
}

pub enum SelectOutput<Output1, Output2> {
    Left(Output1),
    Right(Output2),
}

pub struct Select<S, Fut1, Fut2>
where
    S: Syscalls,
    Fut1: TockFuture<S>,
    Fut2: TockFuture<S>,
{
    fut1: Fut1,
    fut2: Fut2,
    s: PhantomData<S>,
}

impl<S, Fut1, Fut2> TockFuture<S> for Select<S, Fut1, Fut2>
where
    S: Syscalls,
    Fut1: TockFuture<S>,
    Fut2: TockFuture<S>,
{
    type Output = SelectOutput<<Fut1 as TockFuture<S>>::Output, <Fut2 as TockFuture<S>>::Output>;

    fn check_resolved(&self) -> bool {
        self.fut1.check_resolved() || self.fut2.check_resolved()
    }

    fn await_completion(self) -> Self::Output {
        loop {
            if self.fut1.check_resolved() {
                return SelectOutput::Left(self.fut1.await_completion());
            } else if self.fut2.check_resolved() {
                return SelectOutput::Right(self.fut2.await_completion());
            }
            S::yield_wait();
        }
    }
}

pub struct Join<S, Fut1, Fut2>
where
    S: Syscalls,
    Fut1: TockFuture<S>,
    Fut2: TockFuture<S>,
{
    fut1: Fut1,
    fut2: Fut2,
    s: PhantomData<S>,
}

impl<S, Fut1, Fut2> TockFuture<S> for Join<S, Fut1, Fut2>
where
    S: Syscalls,
    Fut1: TockFuture<S>,
    Fut2: TockFuture<S>,
{
    type Output = (
        <Fut1 as TockFuture<S>>::Output,
        <Fut2 as TockFuture<S>>::Output,
    );

    fn check_resolved(&self) -> bool {
        self.fut1.check_resolved() && self.fut2.check_resolved()
    }

    fn await_completion(self) -> Self::Output {
        let output_1 = self.fut1.await_completion();
        let output_2 = self.fut2.await_completion();
        (output_1, output_2)
    }
}

pub struct ReadyFuture<T>(T);

impl<T> ReadyFuture<T> {
    pub fn new(t: T) -> Self {
        Self(t)
    }
}

impl<S: Syscalls, T> TockFuture<S> for ReadyFuture<T> {
    type Output = T;

    fn check_resolved(&self) -> bool {
        true
    }

    fn await_completion(self) -> Self::Output {
        self.0
    }
}

pub struct PendingFuture<T>(PhantomData<T>);

impl<T> PendingFuture<T> {
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl<S: Syscalls, T> TockFuture<S> for PendingFuture<T> {
    type Output = T;

    fn check_resolved(&self) -> bool {
        false
    }

    fn await_completion(self) -> Self::Output {
        panic!("Awaited completion of pending (never finishing) future!");
    }
}
