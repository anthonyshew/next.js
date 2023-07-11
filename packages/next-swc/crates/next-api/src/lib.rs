#![feature(future_join)]
#![feature(min_specialization)]

mod app;
mod pages;
pub mod project;
pub mod route;

pub fn register() {
    next_core::register();
    include!(concat!(env!("OUT_DIR"), "/register.rs"));
}
