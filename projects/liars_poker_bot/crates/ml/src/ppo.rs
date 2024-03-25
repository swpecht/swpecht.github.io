use burn::{
    config::Config,
    module::Module,
    nn::{loss::CrossEntropyLoss, Linear, LinearConfig, ReLU},
    optim::AdamConfig,
    tensor::{backend::Backend, Int, Tensor},
};

/// https://keras.io/examples/rl/ppo_cartpole/
/// https://burn.dev/book/basic-workflow/model.html
///
///

#[derive(Module, Debug)]
pub struct Model<B: Backend> {
    linear1: Linear<B>,
    linear2: Linear<B>,
    activation: ReLU,
}

#[derive(Config, Debug)]
pub struct ModelConfig {
    num_actions: usize,
    hidden_size: usize,
    #[config(default = "0.5")]
    dropout: f64,
}

impl ModelConfig {
    /// Returns the initialized model.
    pub fn init<B: Backend>(&self, device: &B::Device) -> Model<B> {
        Model {
            activation: ReLU::new(),
            linear1: LinearConfig::new(4, self.hidden_size).init(device),
            linear2: LinearConfig::new(self.hidden_size, self.num_actions).init(device),
        }
    }
}

impl<B: Backend> Model<B> {
    /// # Shapes
    ///   - Istate [batch_size, slots]
    ///   - Output [batch_size, num_classes]
    pub fn forward(&self, istate: Tensor<B, 2>) -> Tensor<B, 2> {
        // let [batch_size, istate_actions] = istate.dims();

        let x = istate;

        let x = self.linear1.forward(x);
        let x = self.activation.forward(x);

        self.linear2.forward(x) // [batch_size, num_classes]
    }
}

#[derive(Config)]
pub struct PPOTrainingConfig {
    #[config(default = 42)]
    pub seed: u64,
    #[config(default = 4000)]
    pub steps_per_epoch: usize,
    #[config(default = 30)]
    pub epochs: usize,
    #[config(default = 0.99)]
    pub gamma: f64,
    #[config(default = 0.2)]
    pub clip_ratio: f64,
    #[config(default = 3e-4)]
    pub policy_learning_rate: f64,
    #[config(default = 1e-3)]
    pub value_function_learning_rate: f64,
    #[config(default = 80)]
    pub train_policy_iterations: usize,
    #[config(default = 80)]
    pub train_value_iterations: usize,
    #[config(default = 0.97)]
    pub lam: f64,
    #[config(default = 0.01)]
    pub target_kl: f64,

    pub model: ModelConfig,
    pub ploicy_optimizer: AdamConfig,
    pub value_optimizer: AdamConfig,
}

pub fn run<B: Backend>(device: B::Device) {
    // Create the configuration.
    let config_model = ModelConfig::new(5, 64);
    let config_policy_optimizer = AdamConfig::new();
    let value_optimizer = AdamConfig::new();
    let config = PPOTrainingConfig::new(config_model, config_policy_optimizer, value_optimizer);

    B::seed(config.seed);

    // Create the model and optimizer.
    let mut model = config.model.init(&device);
    let mut policy_optimizer = config.ploicy_optimizer.init();
    let mut value_optimizer = config.value_optimizer.init();

    // // Create the batcher.
    // let batcher_train = MnistBatcher::<B>::new(device.clone());
    // let batcher_valid = MnistBatcher::<B::InnerBackend>::new(device.clone());

    // // Create the dataloaders.
    // let dataloader_train = DataLoaderBuilder::new(batcher_train)
    //     .batch_size(config.batch_size)
    //     .shuffle(config.seed)
    //     .num_workers(config.num_workers)
    //     .build(MnistDataset::train());

    // let dataloader_test = DataLoaderBuilder::new(batcher_valid)
    //     .batch_size(config.batch_size)
    //     .shuffle(config.seed)
    //     .num_workers(config.num_workers)
    //     .build(MnistDataset::test());

    // // Iterate over our training and validation loop for X epochs.
    // for epoch in 1..config.num_epochs + 1 {
    //     // Implement our training loop.
    //     for (iteration, batch) in dataloader_train.iter().enumerate() {
    //         let output = model.forward(batch.images);
    //         let loss = CrossEntropyLoss::new(None, &output.device())
    //             .forward(output.clone(), batch.targets.clone());
    //         let accuracy = accuracy(output, batch.targets);

    //         println!(
    //             "[Train - Epoch {} - Iteration {}] Loss {:.3} | Accuracy {:.3} %",
    //             epoch,
    //             iteration,
    //             loss.clone().into_scalar(),
    //             accuracy,
    //         );

    //         // Gradients for the current backward pass
    //         let grads = loss.backward();
    //         // Gradients linked to each parameter of the model.
    //         let grads = GradientsParams::from_grads(grads, &model);
    //         // Update the model using the optimizer.
    //         model = optim.step(config.lr, model, grads);
    //     }

    //     // Get the model without autodiff.
    //     let model_valid = model.valid();

    //     // Implement our validation loop.
    //     for (iteration, batch) in dataloader_test.iter().enumerate() {
    //         let output = model_valid.forward(batch.images);
    //         let loss = CrossEntropyLoss::new(None, &output.device())
    //             .forward(output.clone(), batch.targets.clone());
    //         let accuracy = accuracy(output, batch.targets);

    //         println!(
    //             "[Valid - Epoch {} - Iteration {}] Loss {} | Accuracy {}",
    //             iteration,
    //             epoch,
    //             loss.clone().into_scalar(),
    //             accuracy,
    //         );
    //     }
    // }
}
