# frozen_string_literal: true

# <code>WIN32OLE_EVENT</code> objects controls OLE event.
class WIN32OLE_EVENT
  # Translates and dispatches Windows message.
  def self.message_loop; end

  # Returns OLE event object.
  # The first argument specifies WIN32OLE object.
  # The second argument specifies OLE event name.
  #    ie = WIN32OLE.new('InternetExplorer.Application')
  #    ev = WIN32OLE_EVENT.new(ie, 'DWebBrowserEvents')
  def initialize(ole, event) end

  # Defines the callback event.
  # If argument is omitted, this method defines the callback of all events.
  #   ie = WIN32OLE.new('InternetExplorer.Application')
  #   ev = WIN32OLE_EVENT.new(ie, 'DWebBrowserEvents')
  #   ev.on_event("NavigateComplete") {|url| puts url}
  def on_event(*event) end

  # Defines the callback of event.
  # If you want modify argument in callback,
  # you should use this method instead of WIN32OLE_EVENT#on_event.
  def on_event_with_outargs(*event) end
end
