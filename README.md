# physics_reinforcement_learning_environment
A physics based reinforcement learning environment with a level editor.

<img width="500" alt="example" src="https://github.com/ShouvikGhosh2048/physics_reinforcement_learning_environment/assets/91585022/708a0f97-8aaa-4f3e-94ed-eeab5b231d12">

## Library
To use the library, add the following to your `Cargo.toml`:
```TOML
physics_reinforcement_learning_environment = { git = "https://github.com/ShouvikGhosh2048/physics_reinforcement_learning_environment.git", tag = "0.3" }
```

Example using the library:
```Rust
use physics_reinforcement_learning_environment::{
    Move, World, Environment,
    egui::{self, Ui}, Sender, Receiver,
    Agent, TrainingDetails, Algorithm, run
};

// We define our agent.
#[derive(Clone)]
pub struct SingleMoveAgent {
    player_move: Move,
}

// We implement the Agent trait for our agent.
impl Agent for SingleMoveAgent {
    fn get_move(&mut self, _environment: &Environment) -> Move {
        self.player_move
    }

    // Show the agent details UI. Uses egui for the UI.
    fn details_ui(&self, ui: &mut Ui, environment: &Environment) {
        ui.label(format!("Move: {:?}", self.player_move));
    }
}

// The training takes places on a seperate thread.
// We define the message we send from the training thread to
// the visualization thread.
// Here we send over the agent and the minimum distance during training.
type SingleMoveMessage = (SingleMoveAgent, f32);

// We define the struct which keeps track of the training details.
struct SingleMoveTrainingDetails {
    agents: Vec<(SingleMoveAgent, f32)>,
    receiver: Receiver<SingleMoveMessage>,
}

// We implement the TrainingDetails trait for the struct.
impl TrainingDetails<SingleMoveAgent, SingleMoveMessage> for SingleMoveTrainingDetails {
    // Receive messages from the training thread.
    fn receive_messages(&mut self) {
        self.agents.extend(self.receiver.try_iter().take(1000));
    }

    // Show the training details UI. Uses egui for the UI.
    // This method returns an Option<&SingleMoveAgent>
    // - if an agent is returned, the app will change to
    // visualize the agent.
    fn details_ui(&mut self, ui: &mut Ui) -> Option<&SingleMoveAgent> {
        let mut selected_agent = None;
        for (agent, score) in self.agents.iter() {
            ui.horizontal(|ui| {
                ui.label(format!("Score {score}"));
                if ui.button("Visualise agent").clicked() {
                    selected_agent = Some(agent);
                }
            });
        }
        selected_agent
    }
}

// We define the struct representing the algorithm.
#[derive(PartialEq, Clone, Copy)]
pub struct SingleMoveAlgorithm {
    number_of_steps: usize,
}

impl Default for SingleMoveAlgorithm {
    fn default() -> Self {
        SingleMoveAlgorithm {
            number_of_steps: 1000,
        }
    }
}

// We implement the Algorithm trait for the struct.
impl Algorithm<SingleMoveAgent, SingleMoveMessage, SingleMoveTrainingDetails> for SingleMoveAlgorithm {
    // Note that the application can drop the receiver when it doesn't
    // want to receive any more messages.
    // The application doesn't stop the training thread - it's your responsibillity
    // to return if you detect that the receiver is dropped.
    fn train(&self, world: World, sender: Sender<SingleMoveMessage>) {
        for left in [false, true] {
            for right in [false, true] {
                for up in [false, true] {
                    let player_move = Move {
                        left,
                        right,
                        up
                    };

                    let (mut environment, _) = Environment::from_world(&world);
                    let mut score = f32::INFINITY;
                    for _ in 0..self.number_of_steps {
                        environment.step(player_move);
                        score = score.min(environment.distance_to_goals().unwrap());
                         
                        if environment.won() {
                            break;
                        }
                    }

                    if sender
                        .send((
                            SingleMoveAgent { player_move },
                            score
                        ))
                        .is_err() {
                        // Can't send a message, so we return.
                        return;
                    }
                }
            }
        }
    }

    // UI for algorithm selection.
    fn selection_ui(&mut self, ui: &mut Ui) {
        ui.label("Number of steps: ");
        ui.add(egui::DragValue::new(&mut self.number_of_steps).clamp_range(1..=10000));    
    }

    // Function which takes a Receiver receiving messages from
    // the training thread and returns the TrainingDetails.
    fn training_details_receiver(
        &self,
        receiver: Receiver<SingleMoveMessage>,
    ) -> SingleMoveTrainingDetails {
        SingleMoveTrainingDetails {
            agents: vec![],
            receiver,
        }
    }
}

fn main() {
    run::<SingleMoveAgent, SingleMoveMessage, SingleMoveTrainingDetails, SingleMoveAlgorithm>();
}
```

Another example using a genetic algorithm is available in `main.rs`.

## Binary
A binary release is available on Github. It contains an implemententation of a genetic algorithm.

## License
The project uses [bevy_github_ci_template](https://github.com/bevyengine/bevy_github_ci_template) for Github workflows and the clippy lint.
The remaining source code is available under the MIT license.
