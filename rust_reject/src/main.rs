#[macro_use]
extern crate log;
extern crate simple_logger;
extern crate stopwatch;
extern crate rand;

use rand::{thread_rng, Rng};
use rand::distributions::{Exp, IndependentSample};

use std::cmp::Ordering;
use std::collections::BinaryHeap;
//use std::collections::HashSet;
use std::iter::FromIterator;

use std::env;
use std::fs;
use std::fs::File;
use std::io::prelude::*;
use std::io::{BufWriter, Write};

use stopwatch::Stopwatch;

type Node = usize;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum State {
    Infected,
    Susceptible,
}
const RECOVERY_RATE: f64 = 1.0;
const INFECTION_RATE: f64 = 0.6;
const HORIZON: f64 = 10.0;
const SAVEINTERVAL: usize = 1000;

struct NodeInfo {
    state: State,
    recovery_time: f64, // only valid if state  is infected
    degree: usize,
}

#[derive(Debug, Copy, Clone)]
struct CountsAtTime {
    infected_count: usize,
    susceptible_count: usize,
    current_time: f64,
}

type Summary = Vec<CountsAtTime>;
type Node2Nodeinfo = Vec<NodeInfo>;
type GraphMap = Vec<Vec<Node>>; // Node to Neighbors


#[derive(PartialEq, Debug, Clone)]
struct Event {
    value: f64,
    src_node: Node,
    target_node: Node,
    src_state: State,
    old_target_state: State,
    new_target_state: State,
}

type EventQueue = BinaryHeap<Event>;

impl Eq for Event {}
impl PartialOrd for Event {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        other.value.partial_cmp(&self.value)
    }
}
impl Ord for Event {
    fn cmp(&self, other: &Event) -> Ordering {
        let ord = self.partial_cmp(other).unwrap();
        match ord {
            Ordering::Greater => Ordering::Less,
            Ordering::Less => Ordering::Greater,
            Ordering::Equal => ord,
        }
    }
}

fn str_2_state(state: String) -> State {
    if state == "S" {
        return State::Susceptible;
    } else if state == "I" {
        return State::Infected;
    } else {
        panic!{"unkown state"};
    }
}



fn draw_exp(rate: f64) -> f64{
    let exp = Exp::new(rate as f64);
    let v = exp.ind_sample(&mut rand::thread_rng());
    return v as f64;
}

fn sample_recovery_time() -> f64 {
    return draw_exp(RECOVERY_RATE);
}

fn get_random_neighbor(graph: &GraphMap, n: usize) -> usize {
    let neighbors = &graph[n];
    let mut rng = rand::thread_rng();
    let sample = rng.gen_range(0, neighbors.len());
    return neighbors[sample];
}

fn sample_infaction_time(degree: usize) -> f64 {
    return draw_exp(degree as f64 * INFECTION_RATE);
}

fn infection_applicable(infection_time: f64, node: usize, node_2_nodeinfo: &Node2Nodeinfo) -> bool {
    if node_2_nodeinfo[node].state == State::Susceptible {
        return true;
    }
    if node_2_nodeinfo[node].recovery_time < infection_time {
        return true;
    }
    return false;
}

fn create_infection_event(
    node: usize,
    node_2_nodeinfo: &Node2Nodeinfo,
    event_queue: &mut EventQueue,
    graph: &GraphMap,
    current_time: f64,
) {
    let mut application_time = current_time;
    loop {
        // to avoid double attack modelling
        assert!(node_2_nodeinfo[node].recovery_time >current_time);
        application_time += sample_infaction_time(node_2_nodeinfo[node].degree);
        if application_time>node_2_nodeinfo[node].recovery_time {
            return;
        }

        let neighbor = get_random_neighbor(graph,node);
        if infection_applicable(application_time, neighbor, node_2_nodeinfo) {
            let inf_event = Event {
                value: application_time,
                src_node: node,
                target_node: neighbor,
                src_state: State::Infected,
                old_target_state: State::Susceptible,
                new_target_state: State::Infected,
            };
            event_queue.push(inf_event);
            return;
        }
    }
}

fn setup_infection_times(node_2_nodeinfo: &mut Node2Nodeinfo, event_queue: &mut EventQueue, graph: &GraphMap) {
    for n in 0..node_2_nodeinfo.len() {
        //let mut node_info = &mut node_2_nodeinfo[n];
        let mut waiting_time = 0.0;
        if node_2_nodeinfo[n].state == State::Infected {
            create_infection_event(n, node_2_nodeinfo, event_queue, graph, 0.0);
        }
    }
}

fn create_recovery_event(
    node: usize,
    node_2_nodeinfo: &mut Node2Nodeinfo,
    event_queue: &mut EventQueue,
    current_time: f64,
) {
    let recovery_time = sample_recovery_time() + current_time;
    let node_info = &mut node_2_nodeinfo[node];
    assert!(node_info.state == State::Infected);
    node_info.recovery_time = recovery_time;
    let rec_event = Event {
        value: recovery_time,
        src_node: node,
        target_node: node,
        src_state: State::Infected,
        old_target_state: State::Infected,
        new_target_state: State::Susceptible,
    };
    event_queue.push(rec_event);
}

fn setup_recovery_times(node_2_nodeinfo: &mut Node2Nodeinfo, event_queue: &mut EventQueue) {
    for n in 0..node_2_nodeinfo.len() {
        if node_2_nodeinfo[n].state == State::Infected {
            create_recovery_event(n, node_2_nodeinfo, event_queue, 0.0);
        }
    }
}

fn setup_graph(
    graphpath: String,
    graph: &mut GraphMap,
    node_infos: &mut Node2Nodeinfo,
    current_counts: &mut CountsAtTime,
) {
    let mut file = match File::open(&graphpath) {
        Err(_why) => panic!("couldn't find graphfile"),
        Ok(file) => file,
    };
    let mut s = String::new();
    match file.read_to_string(&mut s) {
        Err(_why) => panic!("couldn't read graphfile"),
        Ok(_) => (),
    }
    let lines = s.lines();
    let mut label: State;
    let mut counter: usize = 0;
    let mut degree: usize;

    for l in lines {
        if l.len() < 3 {
            continue;
        }
        let line_info: Vec<&str> = l.split(";").collect();

        if line_info[0].to_string().parse::<usize>().unwrap() != counter {
            println!("Wrong order of nodes in input graph");
        }
        counter += 1;
        degree = 0;

        //let v: &str = line_info[0];
        //v = line_info[0].to_string().parse().unwrap();   TODO why is this never used
        label = str_2_state(line_info[1].to_string());
        if label == State::Infected {
            current_counts.infected_count += 1;
        } else {
            current_counts.susceptible_count += 1;
        }
        if line_info[2].len() > 0 {
            // TODO check if only one neighbor
            let neighbors_str: Vec<&str> = line_info[2].split(",").collect();
            degree = neighbors_str.len();
            let neighbors: Vec<Node> = neighbors_str
                .iter()
                .map(|v| v.to_string().parse::<Node>().unwrap())
                .collect();
            graph.push(neighbors);
            node_infos.push(NodeInfo {
                state: label,
                recovery_time: 0.0,
                degree: degree,
            });
        } else {
            println!("Node without neighbour occured{:?}", l);
            //panic!("use kmax larger than 0");
            let neighbors: Vec<Node> = [].to_vec();
            graph.push(neighbors);
            node_infos.push(NodeInfo {
                state: label,
                recovery_time: 0.0,
                degree: degree,
            });
        }
    }
}

fn read_arguments() -> (String, String) {
    let args: Vec<String> = env::args().collect();
    return (args[1].clone(), args[2].clone());
}



fn apply_recovery(current_event: &Event, node_2_nodeinfo: &mut Node2Nodeinfo) {
    let node = current_event.src_node;
    info!("recover! {}", node);
    assert!(node_2_nodeinfo[node].state == State::Infected);
    node_2_nodeinfo[node].state = State::Susceptible;
}

fn apply_infection(current_event: &Event, node_2_nodeinfo: &mut Node2Nodeinfo) -> bool {
    let src_node = current_event.src_node;
    let target_node = current_event.target_node;
    assert!(src_node != target_node);
    assert!(current_event.src_state == State::Infected);
    assert!(node_2_nodeinfo[src_node].state == State::Infected);
    assert!(current_event.old_target_state == State::Susceptible);
    if node_2_nodeinfo[src_node].state == current_event.src_state
        && node_2_nodeinfo[target_node].state == current_event.old_target_state
    {
        info!("infect! {}", target_node);
        node_2_nodeinfo[target_node].state = State::Infected;
        return true;
    }
    return false;
}

fn apply_event(current_event: &Event, node_2_nodeinfo: &mut Node2Nodeinfo) -> bool {
    // if recovery event
    if current_event.new_target_state == State::Susceptible {
        apply_recovery(&current_event, node_2_nodeinfo);
        return true; // is always successful
    } else {
        // is infection event
        let was_successful = apply_infection(&current_event, node_2_nodeinfo);
        return was_successful;
    }
}

fn perform_step(
    graph: &mut GraphMap,
    node_2_nodeinfo: &mut Node2Nodeinfo,
    current_counts: &mut CountsAtTime,
    event_queue: &mut EventQueue,
    current_time: f64,
) -> (f64, bool) {
    // only updates counts not time in current_counts

    if event_queue.len() == 0 {
        info!("No events left. Simualtion over.");
        return (HORIZON + 0.0000001, false);
    }
    let current_event = event_queue.pop().unwrap();
    let current_time = current_event.value;
    info!("{:?}", current_event);

    let was_successful = apply_event(&current_event, node_2_nodeinfo);
    info!("was successful {}", was_successful);


    //current_event.is_infection
    if current_event.new_target_state == State::Infected {
        if was_successful {
            // order is important, recovery before infection event
            create_recovery_event(
                current_event.target_node,
                node_2_nodeinfo,
                event_queue,
                current_time,
            );
            create_infection_event(
                current_event.target_node,
                node_2_nodeinfo,
                event_queue,
                graph,
                current_time,
            );
        }
        create_infection_event(
            current_event.src_node,
            node_2_nodeinfo,
            event_queue,
            graph,
            current_time,
        );
    }

    // successful infection event
    if was_successful && (current_event.new_target_state == State::Infected)  {
        current_counts.susceptible_count -= 1;
        current_counts.infected_count += 1;
    } else if was_successful && (current_event.new_target_state == State::Susceptible) {
        current_counts.susceptible_count += 1;
        current_counts.infected_count -= 1;
        }

    return (current_time, was_successful);
}

fn save_system_state(summary: &mut Summary, current_time: f64, current_counts: CountsAtTime) {
    summary.push(current_counts);
    return;
}

fn write_output(summary: Summary, step_count: usize, rejected_steps: usize, runtime: i64, outpath: String) {
    let outpath_runtime = outpath.replace(".txt", "_runtime.txt");

    let output_rows = 1000;
    let result_len = summary.len() as i32;
    let subsamp_index = result_len/output_rows as i32;
    let mut counter = 0;

    let mut f = BufWriter::new(fs::File::create(outpath_runtime).unwrap());
    write!(f, "runtime(ms),steps,rejected_steps\n{:?},{},{}", runtime, step_count,rejected_steps);

    let mut f = BufWriter::new(fs::File::create(outpath).unwrap());
    write!(f, "state,fraction,time\n");
    for node_count in summary {
        counter += 1;
        if result_len > output_rows*2 && counter > 100 && counter < result_len-100 && counter % subsamp_index != 0  {
            continue;
        }

        let number_of_nodes = (node_count.susceptible_count + node_count.infected_count) as f32;
        let s_frac = (node_count.susceptible_count as f32) / number_of_nodes;
        let i_frac = (node_count.infected_count as f32) / number_of_nodes;
        let time = node_count.current_time;
        write!(f, "S,{},{}\n", s_frac, time);
        write!(f, "I,{},{}\n", i_frac, time);
    }

    return;
}

fn print_event_queue(mut event_queue: EventQueue) {
    while !event_queue.is_empty(){
        println!("{:?}", event_queue.pop());
    }
}

fn main() {
    simple_logger::init_with_level(log::Level::Warn).unwrap();

    let mut graph: GraphMap = Vec::new();
    let mut event_queue: EventQueue = BinaryHeap::new();
    let mut node_2_nodeinfo: Node2Nodeinfo = Vec::new();
    let mut summary: Summary = Vec::with_capacity(100000);
    let (graphpath, outpath) = read_arguments();
    let mut current_counts: CountsAtTime = CountsAtTime {
        infected_count: 0,
        susceptible_count: 0,
        current_time: 0.0,
    };
    setup_graph(
        graphpath,
        &mut graph,
        &mut node_2_nodeinfo,
        &mut current_counts,
    );
    setup_recovery_times(&mut node_2_nodeinfo, &mut event_queue);
    setup_infection_times(&mut node_2_nodeinfo, &mut event_queue, &graph);
    info!("current counts {}", current_counts.susceptible_count);

    //print_event_queue(event_queue.clone());

    let mut current_step: usize = 0;
    let mut current_time: f64 = 0.0;
    let mut real_steps: usize = 0;
    let mut rejected_steps: usize = 0;

    let stopwatch = Stopwatch::start_new();

    while current_time < HORIZON {
        info!("Current counts: {:?}, time: {}", current_counts, current_time);

        if current_step < 100 || current_step % SAVEINTERVAL == 0 {
            save_system_state(&mut summary, current_time, current_counts.clone());
        }

        let (event_time, was_successful) = perform_step(
            &mut graph,
            &mut node_2_nodeinfo,
            &mut current_counts,
            &mut event_queue,
            current_time,
        );
        current_time = event_time;
        current_step += 1;
        if was_successful {
            real_steps += 1;
        } else {
            rejected_steps += 1;
        }

        current_counts.current_time = current_time;

        if current_step % 10000 == 0 {
            print!(".");
            if current_step % 1000000 == 0 {
                println!("\ntime: {}", current_time);
            }
        }
    }

    let elapsed_time = stopwatch.elapsed_ms();
    info!("Number of steps: {}", current_step);
    save_system_state(&mut summary, current_time, current_counts.clone());
    write_output(summary, real_steps, rejected_steps, elapsed_time, outpath);
}
