use crate::{api, config};
use anyhow::{Context, Result};
use colored::Colorize;

pub async fn handle_create_project(name: String) -> Result<()> {
    o_step!("Creating project '{}'...", name.cyan());
    let cfg = config::load_config().context("Config not found. Please log in with `ops login`.")?;
    let token = cfg.token.context("You are not logged in. Please run `ops login` first.")?;

    let res = api::create_project(&token, &name).await?;
    o_success!("{}", format!("✔ {}", res.message).green());
    Ok(())
}

pub async fn handle_list_projects(name_filter: Option<String>) -> Result<()> {
    let cfg = config::load_config().context("Config not found. Please log in with `ops login`.")?;
    let token = cfg.token.context("You are not logged in. Please run `ops login` first.")?;

    let res = api::list_projects(&token, name_filter.as_deref()).await?;

    if res.projects.is_empty() {
        o_result!("No projects found.");
        return Ok(());
    }

    // 绘制树形结构
    o_step!("{}", "Projects Tree".bold().underline());
    
    for (i, project) in res.projects.iter().enumerate() {
        let is_last_project = i == res.projects.len() - 1;
        let p_prefix = if is_last_project { "└──" } else { "├──" };
        
        o_detail!("{} {}", p_prefix, project.name.cyan().bold());

        if project.nodes.is_empty() {
            let n_prefix = if is_last_project { "    └──" } else { "│   └──" };
            o_detail!("{} {}", n_prefix, "(no servers)".dimmed());
        } else {
            for (j, node) in project.nodes.iter().enumerate() {
                let is_last_node = j == project.nodes.len() - 1;
                
                // 确定缩进
                let n_prefix_start = if is_last_project { "    " } else { "│   " };
                let n_tree_char = if is_last_node { "└──" } else { "├──" };
                
                let info = format!("{} ({})", node.environment.yellow(), node.ip_address);
                o_detail!("{}{} {}", n_prefix_start, n_tree_char, info);
            }
        }
    }

    Ok(())
}