# frozen_string_literal: true

# The GC module provides an interface to Ruby's mark and
# sweep garbage collection mechanism.
#
# Some of the underlying methods are also available via the ObjectSpace
# module.
#
# You may obtain information about the operation of the GC through
# GC::Profiler.
module GC
  # The number of times GC occurred.
  #
  # It returns the number of times GC occurred since the process started.
  def self.count; end

  # Disables garbage collection, returning +true+ if garbage
  # collection was already disabled.
  #
  #    GC.disable   #=> false
  #    GC.disable   #=> true
  def self.disable; end

  # Enables garbage collection, returning +true+ if garbage
  # collection was previously disabled.
  #
  #    GC.disable   #=> false
  #    GC.enable    #=> true
  #    GC.enable    #=> false
  def self.enable; end

  # Returns the size of memory allocated by malloc().
  #
  # Only available if ruby was built with +CALC_EXACT_MALLOC_SIZE+.
  def self.malloc_allocated_size; end

  # Returns the number of malloc() allocations.
  #
  # Only available if ruby was built with +CALC_EXACT_MALLOC_SIZE+.
  def self.malloc_allocations; end

  # Initiates garbage collection, unless manually disabled.
  def self.start; end

  # Returns a Hash containing information about the GC.
  #
  # The hash includes information about internal statistics about GC such as:
  #
  #     {
  #         :count=>0,
  #         :heap_used=>12,
  #         :heap_length=>12,
  #         :heap_increment=>0,
  #         :heap_live_num=>7539,
  #         :heap_free_num=>88,
  #         :heap_final_num=>0,
  #         :total_allocated_object=>7630,
  #         :total_freed_object=>88
  #     }
  #
  # The contents of the hash are implementation specific and may be changed in
  # the future.
  #
  # This method is only expected to work on C Ruby.
  def self.stat; end

  # Returns current status of GC stress mode.
  def self.stress; end

  # Updates the GC stress mode.
  #
  # When stress mode is enabled, the GC is invoked at every GC opportunity:
  # all memory and object allocations.
  #
  # Enabling stress mode will degrade performance, it is only for debugging.
  def self.stress=(bool) end

  # Initiates garbage collection, unless manually disabled.
  def garbage_collect; end

  # The GC profiler provides access to information on GC runs including time,
  # length and object space size.
  #
  # Example:
  #
  #   GC::Profiler.enable
  #
  #   require 'rdoc/rdoc'
  #
  #   GC::Profiler.report
  #
  #   GC::Profiler.disable
  #
  # See also GC.count, GC.malloc_allocated_size and GC.malloc_allocations
  module Profiler
    # Clears the GC profiler data.
    def self.clear; end

    # Stops the GC profiler.
    def self.disable; end

    # Starts the GC profiler.
    def self.enable; end

    # The current status of GC profile mode.
    def self.enabled?; end

    # Returns an Array of individual raw profile data Hashes ordered
    # from earliest to latest by +:GC_INVOKE_TIME+.
    #
    # For example:
    #
    #   [
    #     {
    #        :GC_TIME=>1.3000000000000858e-05,
    #        :GC_INVOKE_TIME=>0.010634999999999999,
    #        :HEAP_USE_SIZE=>289640,
    #        :HEAP_TOTAL_SIZE=>588960,
    #        :HEAP_TOTAL_OBJECTS=>14724,
    #        :GC_IS_MARKED=>false
    #     },
    #     # ...
    #   ]
    #
    # The keys mean:
    #
    # +:GC_TIME+::
    #     Time elapsed in seconds for this GC run
    # +:GC_INVOKE_TIME+::
    #     Time elapsed in seconds from startup to when the GC was invoked
    # +:HEAP_USE_SIZE+::
    #     Total bytes of heap used
    # +:HEAP_TOTAL_SIZE+::
    #     Total size of heap in bytes
    # +:HEAP_TOTAL_OBJECTS+::
    #     Total number of objects
    # +:GC_IS_MARKED+::
    #     Returns +true+ if the GC is in mark phase
    #
    # If ruby was built with +GC_PROFILE_MORE_DETAIL+, you will also have access
    # to the following hash keys:
    #
    # +:GC_MARK_TIME+::
    # +:GC_SWEEP_TIME+::
    # +:ALLOCATE_INCREASE+::
    # +:ALLOCATE_LIMIT+::
    # +:HEAP_USE_SLOTS+::
    # +:HEAP_LIVE_OBJECTS+::
    # +:HEAP_FREE_OBJECTS+::
    # +:HAVE_FINALIZE+::
    def self.raw_data; end

    # Writes the GC::Profiler.result to <tt>$stdout</tt> or the given IO object.
    def self.report(*several_variants) end

    # Returns a profile data report such as:
    #
    #   GC 1 invokes.
    #   Index    Invoke Time(sec)       Use Size(byte)     Total Size(byte)         Total Object                    GC time(ms)
    #       1               0.012               159240               212940                10647         0.00000000000001530000
    def self.result; end

    # The total time used for garbage collection in seconds
    def self.total_time; end
  end
end
