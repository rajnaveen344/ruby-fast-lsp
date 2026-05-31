# frozen_string_literal: true

# Init for package syslog
module Syslog
  include Syslog::Constants

  def self.LOG_MASK(p1) end

  def self.LOG_UPTO(p1) end

  # Syslog module methods
  def self.close; end

  def self.facility; end

  def self.ident; end

  def self.inspect; end

  def self.instance; end

  def self.log(*args) end

  def self.mask; end

  def self.mask=(p1) end

  def self.open(p1 = v1, p2 = v2, p3 = v3) end

  def self.open!(*args) end

  def self.opened?; end

  def self.options; end

  def self.reopen(*args) end

  private

  def LOG_MASK(p1) end

  def LOG_UPTO(p1) end

  # Syslog module methods
  def close; end

  def facility; end

  def ident; end

  def inspect; end

  def instance; end

  def log(*args) end

  def mask; end

  def mask=(p1) end

  def open(p1 = v1, p2 = v2, p3 = v3) end

  def opened?; end

  def options; end

  def reopen(*args) end
  alias open! reopen

  module Constants
    def self.LOG_MASK(p1) end

    def self.LOG_UPTO(p1) end

    private

    def LOG_MASK(p1) end

    def LOG_UPTO(p1) end
  end
end
