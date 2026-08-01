require 'sinatra/base'

require 'runtime/logger'
require 'platform/platform_services'
require 'platform/auth/gp_auth'
require 'platform/auth/sinatra'
require 'platform/platform_api'
require 'platform/helpers/cookie_helpers'
require 'platform/tracing/tracing'
require 'runtime/puma_logger'
require 'ddtrace'
require 'ddtrace/contrib/sinatra/tracer'
require 'pm/concurrency'
require 'pm/concurrency/monitoring'
require 'pm/scheduler'
require 'pm/observability/metrics'

# bypassing minimum version check for sinatra instrumentation
module Datadog
  module Contrib
    module Sinatra
      class Integration
        def self.compatible?
          super
        end
      end
    end
  end
end

module GoshPosh
  class Base < Sinatra::Base
    register Rack::GoshPoshAuth::Sinatra

    unless %w[test development devteam].include?(ENV['RACK_ENV'])
      Datadog.configure do |c|
        c.use :sinatra
      end

      register Datadog::Contrib::Sinatra::Tracer
    end

    configure do
      set :log_level, :info
      set :log_file, '/goshposh/log/app.log'
      set :log_console, false
      set :media_root, '/goshposh/data/media'
      set :logging, false
      set :raise_errors, false
      set :show_exceptions, false
      set :dump_errors, true
      set :lock, false
    end

    configure :development do
      set :log_level, :debug
      set :log_console, true
    end

    configure :test do
      set :log_level, :debug
      set :log_file, '/goshposh/log/test_app.log'
      set :log_console, true
      set :media_root, '/goshposh/data/test-media'
    end

    configure :devteam, :qa, :stage, :rc, :production, :ut do
      # On all shared environments except reporting, redirect $stdout and $stderr to sinatra.log.
      # If debug_console mode is enabled, we will also not redirect stdout and stderr to sinatra
      if ENV['MODE'] != 'debug_console'
        sinatra_log = File.new('/goshposh/log/sinatra.log', 'a+')
        $stdout = sinatra_log
        $stdout.sync = true
        $stderr = sinatra_log
        $stderr.sync = true
      end
    end

    configure :reporting do
      # Log all messages to console as well.
      set :log_console, true
    end

    register GoshPosh::Runtime::Logger


    Pm::Concurrency.setup(unstructured_logger: logger)
    Pm::Concurrency::Monitoring::Logging.pm_logger = GoshPosh::Runtime::PmLogger.instance

    Platform::PlatformServices.set_logger(logger)
    Platform::PlatformServices.instance

    ## Setup health metrics generator after service initialization
    Pm::Observability::Metrics::Common::MetricsGenerator.config = {
      logger: logger,
      use_timer_task: true,
      service_name: 'api_app',
      metrics_client_type: Pm::Observability::Metrics::ClientTypes::PROMETHEUS
    }
    # Generate prometheus metrics with prometheus client library every 30 seconds with initial delay of 30 seconds
    Pm::Observability::Metrics::Common::MetricsGenerator.instance.schedule_health_metrics_generation(30) do
      checks = []
      # Added mongo heartbeat checks
      Platform::PlatformServices.instance.client_monitors.each do |client, events|
        events&.each_pair do |_event_name, listener|
          listener&.heartbeat_store&.each_pair do |server, check|
            checks << Pm::Observability::Metrics::Common::Checks.mongo_heartbeat_check('db_heartbeat_monitor_check', server, check, client)
          end
        end
      end
      checks
    end

    # Middleware to annotate logs with traceable ID
    use GoshPosh::TracingMiddleware

    # Authentication middleware to intercept API calls
    use Rack::GoshPoshAuth::GPAuth, logger

    helpers do
      include GoshPosh::Platform::API
      include GoshPosh::Platform::CookieHelpers

      def services
        Platform::PlatformServices.instance
      end

      def access_context
        env['req.access_context'] ||= {}
        env['req.access_context']
      end

      def error_response(http_status_code, error)
        content_type :json
        error_hash = {
          error_type: error.respond_to?(:error_type) ? error.error_type : 'InternalError',
          user_message: error.respond_to?(:user_message) ? error.user_message : nil,
          error_code: error.respond_to?(:error_code) ? error.error_code : nil,
          message: error.respond_to?(:message) ? error.message : nil
        }
        error_hash[:params] = error.params if error.respond_to?(:params)

        [http_status_code,
         GoshPosh::Platform::Json.generate({ error: error_hash })]
      end

      if ENV['APP_ROLES'] && !ENV['RACK_ENV'].index('test')
        roles = ENV['APP_ROLES'].split(',')
        if roles.index('batch') || roles.index('batch-redemptions') || roles.index('batch2') || roles.index('sidekiq-batch-commerce')
          Pm::Scheduler.init({ name: 'batch_jobs',
                               thread_count: GoshPosh::Settings.scheduler.thread_count,
                               thread_priority: GoshPosh::Settings.scheduler.thread_priority,
                               metrics: {
                                 enable_metrics: true,
                                 host: ENV.fetch('DD_AGENT_HOST', '127.0.0.1'),
                                 initial_delay_secs: 5, # initial delay 5 sec
                                 runs_every_secs: 30 # sends stats every 30 sec
                               } })
        end
        if roles.index('batch')
          require 'platform/jobs/all_jobs'
          require 'platform/helpers/user_trigger_helpers'
        end
        # commerce jobs
        if roles.index('batch2')
          require 'platform/jobs/all_batch2_jobs'
        end
        # redemption jobs
        if roles.index('batch-redemptions')
          require 'platform/jobs/all_batch_redemptions_jobs'
        end
        # commerce sidekiq jobs
        if roles.index('sidekiq-batch-commerce')
          require 'sidekiq'
          require 'platform/helpers/user_trigger_helpers'
          require 'platform/jobs/commerce_sidekiq_jobs'
        end
      end
    end
  end
end
