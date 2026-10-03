# frozen_string_literal: true

module Inventory
  # Minimal publish/subscribe used by the warehouse.
  module Events
    def self.included(base)
      base.extend(ClassMethods)
    end

    module ClassMethods
      def event_names
        @event_names ||= []
      end

      def emits(*names)
        names.each do |event|
          event_names << event
          define_method("on_#{event}") do |&handler|
            listeners[event] << handler
            self
          end
        end
      end
    end

    def listeners
      @listeners ||= Hash.new { |hash, key| hash[key] = [] }
    end

    def emit(event, *payload, **options)
      listeners[event].each { |handler| handler.call(*payload, **options) }
    end
  end
end
