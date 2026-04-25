use actix_web::{HttpResponse, Responder, web};
use serde_json::Value;

// 处理 /api/version 请求
pub async fn get_version() -> impl Responder {
    // 读取 git_commits.json 文件
    let json_str = include_str!(concat!(env!("OUT_DIR"), "/git_commits.json"));

    // 解析 JSON
    let v: Value = serde_json::from_str(json_str).unwrap_or_else(|e| {
        log::error!("解析 git_commits.json 失败: {e}");
        // 返回默认值
        serde_json::json!({
            "commits": [],
            "count": 0,
            "version": "0.0.0"
        })
    });

    HttpResponse::Ok().json(v)
}

// 配置路由
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.route("/api/version", web::get().to(get_version));
}
