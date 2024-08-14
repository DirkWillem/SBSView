use eframe::{egui, Frame};
use eframe::egui::{Context, Ui};
use std::collections::LinkedList;
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use pollster::FutureExt;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

pub trait State<A> {
    fn apply(&mut self, action: A);

    fn poll_effects(&mut self) -> LinkedList<A> {
        LinkedList::<A>::new()
    }
}

pub trait View<S, A, PA>
    where
        S: State<A>,
{
    fn state(&mut self) -> &mut S;

    fn view(&mut self, ui: &mut Ui) -> LinkedList<A>;

    fn action_to_parent_action(&self, _: &A) -> Option<PA> {
        None
    }
}

pub trait ChildView<S, A, PA> {
    fn render(&mut self, ui: &mut egui::Ui) -> LinkedList<PA>;
}

impl<T, S: State<A>, A: Sized, PA> ChildView<S, A, PA> for T
    where
        T: View<S, A, PA>,
{
    fn render(&mut self, ui: &mut Ui) -> LinkedList<PA> {
        // Handle effects
        let effect_actions = self.state().poll_effects();
        for action in effect_actions {
            self.state().apply(action);
        }

        // Handle UI
        let actions = self.view(ui);
        let mut parent_actions: LinkedList<PA> = Default::default();

        for action in actions {
            if let Some(pa) = self.action_to_parent_action(&action) {
                parent_actions.push_back(pa);
            }

            self.state().apply(action);
        }

        parent_actions
    }
}

pub trait TopLevelView<S, A>
    where S: State<A>
{
    fn state(&mut self) -> &mut S;

    fn view(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) -> LinkedList<A>;
}

pub trait UpdateTopLevelView<S, A>
    where S: State<A>
{
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame);
}

impl<T, S: State<A>, A: Sized> UpdateTopLevelView<S, A> for T
    where T: TopLevelView<S, A>
{
    fn update(&mut self, ctx: &Context, frame: &mut Frame) {
        // Handle effects
        let effect_actions = self.state().poll_effects();
        for action in effect_actions {
            self.state().apply(action);
        }

        // Handle UI
        let actions = self.view(ctx, frame);
        for action in actions {
            self.state().apply(action);
        }
    }
}

pub struct AsyncProcess<T>
    where T: Send + 'static
{
    join_handle: Option<JoinHandle<T>>,
    done: Arc<AtomicBool>,
}

impl<T> AsyncProcess<T>
    where T: Send + 'static
{
    pub fn new<F>(future: F) -> AsyncProcess<F::Output>
        where
            F: Future + Send + 'static,
            F::Output: Send + 'static,
    {
        let done = Arc::new(AtomicBool::new(false));

        AsyncProcess {
            join_handle: Some(tokio::spawn({
                let done = done.clone();
                async move {
                    let result = future.await;
                    done.store(true, Ordering::SeqCst);

                    result
                }
            })),
            done,
        }
    }

    pub fn is_done(&self) -> bool {
        self.done.load(Ordering::SeqCst)
    }

    pub fn get(&mut self) -> T {
        let join_handle = self.join_handle
            .take()
            .expect("Cannot get result from AsyncProcess more than once");

        let result = join_handle.block_on().unwrap();
        self.done.store(false, Ordering::SeqCst);

        result
    }
}
