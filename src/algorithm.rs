use bevy_egui::egui::Ui;
use crossbeam::channel::{Receiver, Sender};

use crate::{common::Move, Environment, World};

// https://stackoverflow.com/questions/75989070/does-static-in-generic-type-definition-refer-to-the-lifetime-of-the-type-itself

pub trait Agent: Clone + Send + Sync + 'static {
    fn get_move(&mut self, environment: &Environment) -> Move;
    fn details_ui(&self, ui: &mut Ui, environment: &Environment);
}

pub trait TrainingDetails<AgentType: Agent, Message: Send + Sync + 'static>:
    Send + Sync + 'static
{
    fn receive_messages(&mut self);
    fn details_ui(&mut self, ui: &mut Ui) -> Option<&AgentType>;
}

pub trait Algorithm<
    AgentType: Agent,
    Message: Send + Sync + 'static,
    TrainingDetailsType: TrainingDetails<AgentType, Message>,
>: Default + Clone + Send + Sync + 'static
{
    fn selection_ui(&mut self, ui: &mut Ui);
    fn train(&self, world: World, sender: Sender<Message>);
    fn training_details_receiver(&self, receiver: Receiver<Message>) -> TrainingDetailsType;
}
