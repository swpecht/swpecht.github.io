use burn::{
    config::Config,
    module::Module,
    nn::{Linear, LinearConfig},
    optim::AdamConfig,
    tensor::{
        backend::{AutodiffBackend, Backend},
        Tensor,
    },
};
use games::{actions, istate::IStateKey, Action, GameState};
use itertools::Itertools;
use rand::prelude::*;

use crate::stats::{self};

/// https://github.com/openai/spinningup/blob/master/spinup/algos/pytorch/ppo/ppo.py -- best source
/// https://burn.dev/book/basic-workflow/model.html
///
///

#[derive(Module, Debug)]
pub struct MLP<B: Backend> {
    hidden1: Linear<B>,
    hidden2: Linear<B>,
    output: Linear<B>,
}

#[derive(Module, Debug)]
pub struct ActorCriticModel<B: Backend> {
    actor: MLP<B>,
    critic: MLP<B>,
}

#[derive(Config, Debug)]
pub struct ModelConfig {
    istate_size: usize,
    num_actions: usize,
    hidden_size: usize,
}

impl ModelConfig {
    /// Returns the initialized model.
    pub fn init<B: Backend>(&self, device: &B::Device) -> ActorCriticModel<B> {
        ActorCriticModel {
            critic: MLP {
                hidden1: LinearConfig::new(self.istate_size, self.hidden_size).init(device),
                hidden2: LinearConfig::new(self.hidden_size, self.hidden_size).init(device),
                output: LinearConfig::new(self.hidden_size, 1).init(device),
            },
            actor: MLP {
                hidden1: LinearConfig::new(self.istate_size, self.hidden_size).init(device),
                hidden2: LinearConfig::new(self.hidden_size, self.hidden_size).init(device),
                output: LinearConfig::new(self.hidden_size, self.num_actions).init(device),
            },
        }
    }
}

impl<B: Backend> MLP<B> {
    pub fn forward(&self, istate: Tensor<B, 2>) -> Tensor<B, 2> {
        let x = istate;
        let x = self.hidden1.forward(x).tanh();
        let x = self.hidden2.forward(x).tanh();
        self.output.forward(x)
    }
}

impl<B: Backend> ActorCriticModel<B> {
    pub fn pi(&self, istates: Tensor<B, 2>, actions: Tensor<B, 2>) {
        todo!()
    }
}

#[derive(Config)]
pub struct PPOTrainingConfig {
    #[config(default = 42)]
    pub seed: u64,
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

    pub ac_model: ModelConfig,
    pub pi_optimizer: AdamConfig,
    pub vf_optimizer: AdamConfig,
}

/// Buffer for storing trajectories
struct PPOBuffer {
    gamma: f32,
    lam: f32,
    observations: Vec<IStateKey>,
    actions: Vec<Action>,
    advantages: Vec<f32>,
    rewards: Vec<f32>,
    returns: Vec<f32>,
    values: Vec<f32>,
    logprobs: Vec<f32>,
}

impl Default for PPOBuffer {
    fn default() -> Self {
        Self {
            gamma: 0.99,
            lam: 0.95,
            actions: Default::default(),
            advantages: Default::default(),
            rewards: Default::default(),
            returns: Default::default(),
            values: Default::default(),
            logprobs: Default::default(),
            observations: Default::default(),
        }
    }
}

impl PPOBuffer {
    pub fn store(
        &mut self,
        observation: IStateKey,
        action: Action,
        reward: f32,
        value: f32,
        logprob: f32,
    ) {
        self.observations.push(observation);
        self.actions.push(action);
        self.rewards.push(reward);
        self.values.push(value);
        self.logprobs.push(logprob);
    }

    /// Finish the trajectory by computing advantage estimates and rewards-to-go
    pub fn finish_trajectory(&mut self, last_value: f32) {
        self.rewards.push(last_value);
        self.values.push(last_value);

        //     deltas = rewards[:-1] + self.gamma * values[1:] - values[:-1]
        let deltas = self
            .rewards
            .iter()
            .zip(self.values.iter().skip(1))
            .zip(self.values.iter())
            .map(|((r, v_next), v)| *r + self.gamma * v_next - v)
            .collect_vec();

        self.advantages = discounted_cumulative_sums(&deltas, self.gamma * self.lam);
        self.returns = discounted_cumulative_sums(&self.rewards, self.gamma);
        self.returns.pop(); // don't need this final value -- makes it same length as advantages
    }

    pub fn get(&mut self) -> (Vec<IStateKey>, Vec<Action>, Vec<f32>, Vec<f32>, Vec<f32>) {
        // normalize advantages
        let advantage_mean = stats::mean(&self.advantages).unwrap();
        let advantage_std = stats::std_deviation(&self.advantages).unwrap();
        self.advantages
            .iter_mut()
            .for_each(|x| *x = (*x - advantage_mean) / advantage_std);

        return (
            self.observations,
            self.actions,
            self.advantages,
            self.returns,
            self.logprobs,
        );
    }
}

pub fn run<B: AutodiffBackend, G: GameState>(device: B::Device, generator: fn() -> G) {
    // Create the configuration.
    let config_model = ModelConfig::new(5, 2, 64);
    let config_actor_optimizer = AdamConfig::new();
    let config_critic_optimizer = AdamConfig::new();
    let config = PPOTrainingConfig::new(
        config_model,
        config_actor_optimizer,
        config_critic_optimizer,
    );

    B::seed(config.seed);
    let mut rng: StdRng = SeedableRng::seed_from_u64(config.seed);

    // Create the model and optimizer.
    let mut ac = config.ac_model.init(&device);
    let mut actor_optimizer = config.pi_optimizer.init();
    let mut critic_optimizer = config.vf_optimizer.init();

    let mut buffer = PPOBuffer::default();

    for epoch in 0..config.epochs {
        // Initialize the sum of the returns, lengths and number of episodes for each epoch
        let mut sum_return = 0.0;
        let mut num_episodes = 0;

        let mut gs = &mut generator();
        deal_game(gs, &mut rng);

        while !gs.is_terminal() {
            // Get the logits, action, and take one step in the environment
            let istate = gs.istate_key(gs.cur_player());
            let logits = get_logits(&istate, &actor_model);
            let action = sample_action(logits);
            gs.apply_action(action);

            // Get the value and log-probability of the action
            let value_t = critic(&istate, &critic_model) as f32;
            let logprobability_t = logprobabilities(logits);

            let reward = 0.0; // todo, fix this -- how should we figure out reward, change which team we're optimizing for each epoch?

            // Store obs, act, rew, v_t, logp_pi_t
            // TODO - should this store all actions?
            buffer.store(istate, action, reward, value_t, logprob_t);
        }

        // Finish trajectory if reached to a terminal state
        //             last_value = 0 if done else critic(observation.reshape(1, -1))
        //             buffer.finish_trajectory(last_value)
        //             sum_return += episode_return
        //             num_episodes += 1
        //             observation, _ = env.reset()
        //             episode_return, episode_length = 0, 0

        //     # Get values from the buffer
        //     (
        //         observation_buffer,
        //         action_buffer,
        //         advantage_buffer,
        //         return_buffer,
        //         logprobability_buffer,
        //     ) = buffer.get()

        // Update the policy and implement early stopping using KL divergence
        for _ in 0..config.train_policy_iterations {
            //         kl = train_policy(
            //             observation_buffer, action_buffer, logprobability_buffer, advantage_buffer
            //         )
            //         if kl > 1.5 * target_kl:
            //             # Early Stopping
            //             break
        }

        // Update the value function
        for _ in 0..config.train_value_iterations {
            //         train_value_function(observation_buffer, return_buffer)
        }

        // Print mean return and length for each epoch
        println!(
            " Epoch: {epoch}. Mean Return: {}.",
            sum_return / num_episodes as f64
        )
    }
}

fn deal_game<G: GameState>(gs: &mut G, rng: &mut StdRng) {
    while gs.is_chance_node() {
        let actions = actions!(gs);
        let a = actions.choose(rng).unwrap();
        gs.apply_action(*a);
    }
}

/// Returns the (action, logit)
fn get_logits<B: Backend, G: GameState>(
    istate: &IStateKey,
    actor_model: &MLP<B>,
) -> Vec<(Action, f64)> {
    // let logits = policy_model.forward(istate);
    // let vec: Vec<f64> = logits.into();
    todo!()
}

fn sample_action(logits: Vec<(Action, f64)>) -> Action {
    todo!()
}

fn train_policy() -> f64 {
    todo!()
    // def train_policy(
    //     observation_buffer, action_buffer, logprobability_buffer, advantage_buffer
    // ):
    //     with tf.GradientTape() as tape:  # Record operations for automatic differentiation.
    //         ratio = keras.ops.exp(
    //             logprobabilities(actor(observation_buffer), action_buffer)
    //             - logprobability_buffer
    //         )
    //         min_advantage = keras.ops.where(
    //             advantage_buffer > 0,
    //             (1 + clip_ratio) * advantage_buffer,
    //             (1 - clip_ratio) * advantage_buffer,
    //         )

    //         policy_loss = -keras.ops.mean(
    //             keras.ops.minimum(ratio * advantage_buffer, min_advantage)
    //         )
    //     policy_grads = tape.gradient(policy_loss, actor.trainable_variables)
    //     policy_optimizer.apply_gradients(zip(policy_grads, actor.trainable_variables))

    //     kl = keras.ops.mean(
    //         logprobability_buffer
    //         - logprobabilities(actor(observation_buffer), action_buffer)
    //     )
    //     kl = keras.ops.sum(kl)
    //     return kl
}

fn critic<B: Backend>(istate: &IStateKey, critic_model: &MLP<B>) -> f64 {
    todo!()
}

fn logprobabilities(logits: Vec<(Action, f64)>) -> Vec<(Action, f64)> {
    todo!()
}

/// Discounted cumulative sums of vectors for computing rewards-to-go and advantage estimates
///
///     input:
///     vector x,
///     [x0,
///      x1,
///      x2]
///
///     output:
///     [x0 + discount * x1 + discount^2 * x2,
///      x1 + discount * x2,
///      x2]
fn discounted_cumulative_sums(x: &[f32], discount: f32) -> Vec<f32> {
    let mut sums = Vec::with_capacity(x.len());
    let discounts = (0..x.len()).map(|i| discount.powi(i as i32));

    for i in 0..x.len() {
        let v = x.iter().skip(i).zip(discounts).map(|(xi, d)| xi * d).sum();
        sums.push(v);
    }
    sums
}

fn compute_loss_pi() {
    // # Set up function for computing PPO policy loss
    // def compute_loss_pi(data):
    //     obs, act, adv, logp_old = data['obs'], data['act'], data['adv'], data['logp']

    //     # Policy loss
    //     pi, logp = ac.pi(obs, act)
    //     ratio = torch.exp(logp - logp_old)
    //     clip_adv = torch.clamp(ratio, 1-clip_ratio, 1+clip_ratio) * adv
    //     loss_pi = -(torch.min(ratio * adv, clip_adv)).mean()

    //     # Useful extra info
    //     approx_kl = (logp_old - logp).mean().item()
    //     ent = pi.entropy().mean().item()
    //     clipped = ratio.gt(1+clip_ratio) | ratio.lt(1-clip_ratio)
    //     clipfrac = torch.as_tensor(clipped, dtype=torch.float32).mean().item()
    //     pi_info = dict(kl=approx_kl, ent=ent, cf=clipfrac)

    //     return loss_pi, pi_info
}

fn compute_loss_v() {
    // # Set up function for computing value loss
    // def compute_loss_v(data):
    //     obs, ret = data['obs'], data['ret']
    //     return ((ac.v(obs) - ret)**2).mean()
}

fn update() {
    // def update():
    // data = buf.get()

    // pi_l_old, pi_info_old = compute_loss_pi(data)
    // pi_l_old = pi_l_old.item()
    // v_l_old = compute_loss_v(data).item()

    // # Train policy with multiple steps of gradient descent
    // for i in range(train_pi_iters):
    //     pi_optimizer.zero_grad()
    //     loss_pi, pi_info = compute_loss_pi(data)
    //     kl = mpi_avg(pi_info['kl'])
    //     if kl > 1.5 * target_kl:
    //         logger.log('Early stopping at step %d due to reaching max kl.'%i)
    //         break
    //     loss_pi.backward()
    //     mpi_avg_grads(ac.pi)    # average grads across MPI processes
    //     pi_optimizer.step()

    // logger.store(StopIter=i)

    // # Value function learning
    // for i in range(train_v_iters):
    //     vf_optimizer.zero_grad()
    //     loss_v = compute_loss_v(data)
    //     loss_v.backward()
    //     mpi_avg_grads(ac.v)    # average grads across MPI processes
    //     vf_optimizer.step()

    // # Log changes from update
    // kl, ent, cf = pi_info['kl'], pi_info_old['ent'], pi_info['cf']
    // logger.store(LossPi=pi_l_old, LossV=v_l_old,
    //              KL=kl, Entropy=ent, ClipFrac=cf,
    //              DeltaLossPi=(loss_pi.item() - pi_l_old),
    //              DeltaLossV=(loss_v.item() - v_l_old))
}
