use callback::CallbackSubscription;
use callback::SubscribableCallback;
use core::slice;
use syscalls;

const DRIVER_NUMBER: usize = 0x10000;

mod ipc_commands {
    pub const REGISTER_SERVICE: usize = 0;
    pub const NOTIFY_CLIENT: usize = 1;
}

pub struct IpcServerCallback<CB> {
    callback: CB,
}

impl<CB: FnMut(usize, usize, &mut [u8])> SubscribableCallback for IpcServerCallback<CB> {
    fn driver_number(&self) -> usize {
        DRIVER_NUMBER
    }

    fn subscribe_number(&self) -> usize {
        ipc_commands::REGISTER_SERVICE
    }

    fn call_rust(&mut self, arg0: usize, arg1: usize, arg2: usize) {
        let mut v = unsafe { slice::from_raw_parts_mut(arg2 as *mut u8, arg1) };
        (self.callback)(arg0, arg1, &mut v);
    }
}

pub fn notify_client(pid: usize) {
    unsafe { syscalls::command(DRIVER_NUMBER, pid, ipc_commands::NOTIFY_CLIENT, 0) };
}

pub struct IpcServerDriver;

impl IpcServerDriver {
    pub fn start<CB: FnMut(usize, usize, &mut [u8])>(
        callback: CB,
    ) -> Result<CallbackSubscription<IpcServerCallback<CB>>, ()> {
        let (_, subscription) = syscalls::subscribe_new(IpcServerCallback { callback });
        Ok(subscription)
    }
}
