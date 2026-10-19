// Exercise the hook's production selection/resolution code in the host-test scope.
// This does not inject into a game or depend on a Linux installation.
#[path = "../hook/src/proxy.rs"]
mod proxy;

#[cfg(all(windows, feature = "hook"))]
#[path = "hook_proxy/native.rs"]
mod native;
