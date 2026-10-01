//! Sample Ruby project for profiling

use std::fs;
use std::path::{Path, PathBuf};

/// Creates a sample Ruby project in a temporary directory
pub fn create_sample_project() -> std::io::Result<PathBuf> {
    let temp_dir = std::env::temp_dir().join("ruby_fast_lsp_profiler_sample");

    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir)?;
    }

    fs::create_dir_all(&temp_dir)?;

    let dirs = ["app/models", "app/services", "app/controllers", "lib"];
    for dir in dirs {
        fs::create_dir_all(temp_dir.join(dir))?;
    }

    create_base_model(&temp_dir)?;
    create_user_model(&temp_dir)?;
    create_post_model(&temp_dir)?;
    create_comment_model(&temp_dir)?;
    create_user_service(&temp_dir)?;
    create_post_service(&temp_dir)?;
    create_users_controller(&temp_dir)?;
    create_posts_controller(&temp_dir)?;
    create_helpers(&temp_dir)?;
    create_additional_models(&temp_dir, 20)?;
    create_additional_services(&temp_dir, 10)?;

    Ok(temp_dir)
}

/// Clean up the sample project
pub fn cleanup_sample_project() -> std::io::Result<()> {
    let temp_dir = std::env::temp_dir().join("ruby_fast_lsp_profiler_sample");
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir)?;
    }
    Ok(())
}

fn create_base_model(root: &Path) -> std::io::Result<()> {
    fs::write(
        root.join("app/models/application_record.rb"),
        r#"class ApplicationRecord
  def self.find(id)
    new
  end

  def self.where(conditions)
    []
  end

  def self.all
    []
  end

  def save
    true
  end

  def update(attrs)
    true
  end

  def destroy
    true
  end

  def valid?
    true
  end

  def errors
    []
  end

  def id
    1
  end

  def created_at
    Time.now
  end

  def updated_at
    Time.now
  end
end
"#,
    )
}

fn create_user_model(root: &Path) -> std::io::Result<()> {
    fs::write(
        root.join("app/models/user.rb"),
        r#"class User < ApplicationRecord
  attr_accessor :name, :email, :age

  def initialize(name: nil, email: nil, age: nil)
    @name = name
    @email = email
    @age = age
  end

  def full_name
    @name.to_s
  end

  def adult?
    @age && @age >= 18
  end

  def posts
    Post.where(user_id: id)
  end

  def comments
    Comment.where(user_id: id)
  end

  def recent_posts(limit = 10)
    posts.take(limit)
  end

  def post_count
    posts.length
  end

  def self.find_by_email(email)
    where(email: email).first
  end

  def self.active
    where(active: true)
  end

  def self.adults
    all.select(&:adult?)
  end

  def greeting
    if adult?
      "Hello!"
    else
      "Hi there!"
    end
  end

  def profile_data
    {
      name: full_name,
      email: @email,
      posts: post_count,
      adult: adult?
    }
  end
end
"#,
    )
}

fn create_post_model(root: &Path) -> std::io::Result<()> {
    fs::write(
        root.join("app/models/post.rb"),
        r#"class Post < ApplicationRecord
  attr_accessor :title, :body, :user_id, :published

  def initialize(title: nil, body: nil, user_id: nil)
    @title = title
    @body = body
    @user_id = user_id
    @published = false
  end

  def author
    User.find(@user_id)
  end

  def comments
    Comment.where(post_id: id)
  end

  def comment_count
    comments.length
  end

  def publish!
    @published = true
    save
  end

  def published?
    @published == true
  end

  def summary(length = 100)
    if @body && @body.length > length
      @body[0...length]
    else
      @body
    end
  end

  def self.published
    where(published: true)
  end

  def self.drafts
    where(published: false)
  end

  def self.by_user(user)
    where(user_id: user.id)
  end

  def self.recent(limit = 10)
    all.take(limit)
  end

  def metadata
    {
      title: @title,
      author: author.full_name,
      comments: comment_count,
      published: published?
    }
  end
end
"#,
    )
}

fn create_comment_model(root: &Path) -> std::io::Result<()> {
    fs::write(
        root.join("app/models/comment.rb"),
        r#"class Comment < ApplicationRecord
  attr_accessor :body, :user_id, :post_id

  def initialize(body: nil, user_id: nil, post_id: nil)
    @body = body
    @user_id = user_id
    @post_id = post_id
  end

  def author
    User.find(@user_id)
  end

  def post
    Post.find(@post_id)
  end

  def short_body(length = 50)
    if @body && @body.length > length
      @body[0...length]
    else
      @body
    end
  end

  def self.by_user(user)
    where(user_id: user.id)
  end

  def self.for_post(post)
    where(post_id: post.id)
  end

  def self.recent(limit = 20)
    all.take(limit)
  end
end
"#,
    )
}

fn create_user_service(root: &Path) -> std::io::Result<()> {
    fs::write(
        root.join("app/services/user_service.rb"),
        r#"class UserService
  def initialize(user = nil)
    @user = user
  end

  def create_user(params)
    user = User.new(
      name: params[:name],
      email: params[:email],
      age: params[:age]
    )
    if user.valid?
      user.save
      user
    else
      nil
    end
  end

  def update_user(params)
    return nil unless @user
    @user.name = params[:name] if params[:name]
    @user.email = params[:email] if params[:email]
    @user.age = params[:age] if params[:age]
    @user.save
    @user
  end

  def delete_user
    return false unless @user
    @user.destroy
  end

  def find_user(id)
    User.find(id)
  end

  def list_users
    User.all
  end

  def active_users
    User.active
  end

  def user_stats
    users = list_users
    {
      total: users.length,
      active: active_users.length,
      adults: User.adults.length
    }
  end

  def self.authenticate(email, password)
    user = User.find_by_email(email)
    user ? user : nil
  end
end
"#,
    )
}

fn create_post_service(root: &Path) -> std::io::Result<()> {
    fs::write(
        root.join("app/services/post_service.rb"),
        r#"class PostService
  def initialize(user = nil)
    @user = user
  end

  def create_post(params)
    post = Post.new(
      title: params[:title],
      body: params[:body],
      user_id: @user&.id
    )
    if post.valid?
      post.save
      post
    else
      nil
    end
  end

  def update_post(post, params)
    post.title = params[:title] if params[:title]
    post.body = params[:body] if params[:body]
    post.save
    post
  end

  def delete_post(post)
    post.destroy
  end

  def publish_post(post)
    post.publish!
  end

  def list_posts
    @user ? Post.by_user(@user) : Post.all
  end

  def published_posts
    Post.published
  end

  def draft_posts
    Post.drafts
  end

  def recent_posts(limit = 10)
    Post.recent(limit)
  end

  def post_stats
    {
      total: Post.all.length,
      published: published_posts.length,
      drafts: draft_posts.length
    }
  end

  def self.trending(limit = 5)
    Post.published.take(limit)
  end
end
"#,
    )
}

fn create_users_controller(root: &Path) -> std::io::Result<()> {
    fs::write(
        root.join("app/controllers/users_controller.rb"),
        r#"class UsersController
  def initialize
    @service = UserService.new
  end

  def index
    users = @service.list_users
    render_json(users)
  end

  def show(id)
    user = @service.find_user(id)
    user ? render_json(user.profile_data) : render_error("User not found", 404)
  end

  def create(params)
    user = @service.create_user(params)
    user ? render_json(user, 201) : render_error("Failed to create user", 422)
  end

  def update(id, params)
    user = @service.find_user(id)
    return render_error("User not found", 404) unless user
    service = UserService.new(user)
    updated = service.update_user(params)
    render_json(updated)
  end

  def destroy(id)
    user = @service.find_user(id)
    return render_error("User not found", 404) unless user
    service = UserService.new(user)
    service.delete_user ? render_json({ deleted: true }) : render_error("Failed", 500)
  end

  def stats
    render_json(@service.user_stats)
  end

  private

  def render_json(data, status = 200)
    { status: status, body: data }
  end

  def render_error(message, status)
    { status: status, error: message }
  end
end
"#,
    )
}

fn create_posts_controller(root: &Path) -> std::io::Result<()> {
    fs::write(
        root.join("app/controllers/posts_controller.rb"),
        r#"class PostsController
  def initialize(current_user = nil)
    @current_user = current_user
    @service = PostService.new(current_user)
  end

  def index
    posts = @service.list_posts
    render_json(posts.map(&:metadata))
  end

  def show(id)
    post = Post.find(id)
    post ? render_json(post.metadata) : render_error("Post not found", 404)
  end

  def create(params)
    post = @service.create_post(params)
    post ? render_json(post.metadata, 201) : render_error("Failed to create post", 422)
  end

  def update(id, params)
    post = Post.find(id)
    return render_error("Post not found", 404) unless post
    updated = @service.update_post(post, params)
    render_json(updated.metadata)
  end

  def destroy(id)
    post = Post.find(id)
    return render_error("Post not found", 404) unless post
    @service.delete_post(post) ? render_json({ deleted: true }) : render_error("Failed", 500)
  end

  def publish(id)
    post = Post.find(id)
    return render_error("Post not found", 404) unless post
    @service.publish_post(post)
    render_json(post.metadata)
  end

  def stats
    render_json(@service.post_stats)
  end

  def trending
    posts = PostService.trending(10)
    render_json(posts.map(&:metadata))
  end

  private

  def render_json(data, status = 200)
    { status: status, body: data }
  end

  def render_error(message, status)
    { status: status, error: message }
  end
end
"#,
    )
}

fn create_helpers(root: &Path) -> std::io::Result<()> {
    fs::write(
        root.join("lib/helpers.rb"),
        r#"module Helpers
  def self.format_date(time)
    time.strftime("%Y-%m-%d")
  end

  def self.format_datetime(time)
    time.strftime("%Y-%m-%d %H:%M:%S")
  end

  def self.truncate(text, length = 100)
    text && text.length > length ? text[0...length] : text
  end

  def self.titleize(text)
    text.split.map(&:capitalize).join(" ")
  end
end

module StringExtensions
  def blank?
    nil? || empty? || strip.empty?
  end

  def present?
    !blank?
  end

  def squish
    strip
  end
end

module ArrayExtensions
  def second
    self[1]
  end

  def third
    self[2]
  end

  def average
    return 0 if empty?
    sum.to_f / length
  end

  def pluck(key)
    map { |item| item[key] }
  end
end
"#,
    )
}

fn create_additional_models(root: &Path, count: usize) -> std::io::Result<()> {
    for i in 0..count {
        let content = format!(
            r#"class Model{i} < ApplicationRecord
  attr_accessor :name, :value, :status

  def initialize(name: nil, value: nil)
    @name = name
    @value = value
    @status = :pending
  end

  def process
    @status = :processing
    result = compute_value
    @status = :completed
    result
  end

  def compute_value
    @value ? @value * 2 : 0
  end

  def formatted_name
    @name.to_s
  end

  def active?
    @status == :completed
  end

  def pending?
    @status == :pending
  end

  def to_hash
    {{ name: @name, value: @value, status: @status, active: active? }}
  end

  def self.find_by_name(name)
    where(name: name).first
  end

  def self.active
    all.select(&:active?)
  end

  def self.pending
    all.select(&:pending?)
  end

  def self.process_all
    pending.each(&:process)
  end
end
"#,
            i = i
        );
        fs::write(
            root.join("app/models").join(format!("model{}.rb", i)),
            content,
        )?;
    }
    Ok(())
}

fn create_additional_services(root: &Path, count: usize) -> std::io::Result<()> {
    for i in 0..count {
        let content = format!(
            r#"class Service{i}
  def initialize(config = {{}})
    @config = config
    @results = []
  end

  def execute(input)
    validate_input(input)
    result = process_input(input)
    store_result(result)
    result
  end

  def validate_input(input)
    raise "Invalid input" if input.nil?
    true
  end

  def process_input(input)
    case input
    when String then input.upcase
    when Integer then input * 2
    else input.to_s
    end
  end

  def store_result(result)
    @results << result
  end

  def results
    @results
  end

  def result_count
    @results.length
  end

  def clear_results
    @results = []
  end

  def last_result
    @results.last
  end

  def config
    @config
  end

  def update_config(new_config)
    @config.merge!(new_config)
  end

  def self.create_with_defaults
    new(timeout: 30, retries: 3)
  end

  def self.batch_execute(inputs)
    service = new
    inputs.map {{ |input| service.execute(input) }}
  end
end
"#,
            i = i
        );
        fs::write(
            root.join("app/services").join(format!("service{}.rb", i)),
            content,
        )?;
    }
    Ok(())
}
