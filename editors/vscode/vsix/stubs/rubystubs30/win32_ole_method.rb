# frozen_string_literal: true

# <code>WIN32OLE_METHOD</code> objects represent OLE method information.
class WIN32OLE_METHOD
  # Returns a new WIN32OLE_METHOD object which represents the information
  # about OLE method.
  # The first argument <i>ole_type</i> specifies WIN32OLE_TYPE object.
  # The second argument <i>method</i> specifies OLE method name defined OLE class
  # which represents WIN32OLE_TYPE object.
  #
  #    tobj = WIN32OLE_TYPE.new('Microsoft Excel 9.0 Object Library', 'Workbook')
  #    method = WIN32OLE_METHOD.new(tobj, 'SaveAs')
  def initialize(ole_type, method) end

  # Returns dispatch ID.
  #    tobj = WIN32OLE_TYPE.new('Microsoft Excel 9.0 Object Library', 'Workbooks')
  #    method = WIN32OLE_METHOD.new(tobj, 'Add')
  #    puts method.dispid # => 181
  def dispid; end

  # Returns true if the method is event.
  #    tobj = WIN32OLE_TYPE.new('Microsoft Excel 9.0 Object Library', 'Workbook')
  #    method = WIN32OLE_METHOD.new(tobj, 'SheetActivate')
  #    puts method.event? # => true
  def event?; end

  # Returns event interface name if the method is event.
  #   tobj = WIN32OLE_TYPE.new('Microsoft Excel 9.0 Object Library', 'Workbook')
  #   method = WIN32OLE_METHOD.new(tobj, 'SheetActivate')
  #   puts method.event_interface # =>  WorkbookEvents
  def event_interface; end

  # Returns help context.
  #    tobj = WIN32OLE_TYPE.new('Microsoft Excel 9.0 Object Library', 'Workbooks')
  #    method = WIN32OLE_METHOD.new(tobj, 'Add')
  #    puts method.helpcontext # => 65717
  def helpcontext; end

  # Returns help file. If help file is not found, then
  # the method returns nil.
  #    tobj = WIN32OLE_TYPE.new('Microsoft Excel 9.0 Object Library', 'Workbooks')
  #    method = WIN32OLE_METHOD.new(tobj, 'Add')
  #    puts method.helpfile # => C:\...\VBAXL9.CHM
  def helpfile; end

  # Returns help string of OLE method. If the help string is not found,
  # then the method returns nil.
  #    tobj = WIN32OLE_TYPE.new('Microsoft Internet Controls', 'IWebBrowser')
  #    method = WIN32OLE_METHOD.new(tobj, 'Navigate')
  #    puts method.helpstring # => Navigates to a URL or file.
  def helpstring; end

  # Returns the method name with class name.
  def inspect; end

  # Returns the method invoke kind.
  #   tobj = WIN32OLE_TYPE.new('Microsoft Excel 9.0 Object Library', 'Workbooks')
  #   method = WIN32OLE_METHOD.new(tobj, 'Add')
  #   puts method.invkind # => 1
  def invkind; end

  # Returns the method kind string. The string is "UNKNOWN" or "PROPERTY"
  # or "PROPERTY" or "PROPERTYGET" or "PROPERTYPUT" or "PROPERTYPPUTREF"
  # or "FUNC".
  #    tobj = WIN32OLE_TYPE.new('Microsoft Excel 9.0 Object Library', 'Workbooks')
  #    method = WIN32OLE_METHOD.new(tobj, 'Add')
  #    puts method.invoke_kind # => "FUNC"
  def invoke_kind; end

  # Returns the name of the method.
  #
  #    tobj = WIN32OLE_TYPE.new('Microsoft Excel 9.0 Object Library', 'Workbook')
  #    method = WIN32OLE_METHOD.new(tobj, 'SaveAs')
  #    puts method.name # => SaveAs
  def name; end
  alias to_s name

  # Returns the offset ov VTBL.
  #    tobj = WIN32OLE_TYPE.new('Microsoft Excel 9.0 Object Library', 'Workbooks')
  #    method = WIN32OLE_METHOD.new(tobj, 'Add')
  #    puts method.offset_vtbl # => 40
  def offset_vtbl; end

  # returns array of WIN32OLE_PARAM object corresponding with method parameters.
  #    tobj = WIN32OLE_TYPE.new('Microsoft Excel 9.0 Object Library', 'Workbook')
  #    method = WIN32OLE_METHOD.new(tobj, 'SaveAs')
  #    p method.params # => [Filename, FileFormat, Password, WriteResPassword,
  #                          ReadOnlyRecommended, CreateBackup, AccessMode,
  #                          ConflictResolution, AddToMru, TextCodepage,
  #                          TextVisualLayout]
  def params; end

  # Returns string of return value type of method.
  #    tobj = WIN32OLE_TYPE.new('Microsoft Excel 9.0 Object Library', 'Workbooks')
  #    method = WIN32OLE_METHOD.new(tobj, 'Add')
  #    puts method.return_type # => Workbook
  def return_type; end

  # Returns detail information of return value type of method.
  # The information is array.
  #    tobj = WIN32OLE_TYPE.new('Microsoft Excel 9.0 Object Library', 'Workbooks')
  #    method = WIN32OLE_METHOD.new(tobj, 'Add')
  #    p method.return_type_detail # => ["PTR", "USERDEFINED", "Workbook"]
  def return_type_detail; end

  # Returns number of return value type of method.
  #    tobj = WIN32OLE_TYPE.new('Microsoft Excel 9.0 Object Library', 'Workbooks')
  #    method = WIN32OLE_METHOD.new(tobj, 'Add')
  #    puts method.return_vtype # => 26
  def return_vtype; end

  # Returns the size of optional parameters.
  #    tobj = WIN32OLE_TYPE.new('Microsoft Excel 9.0 Object Library', 'Workbook')
  #    method = WIN32OLE_METHOD.new(tobj, 'SaveAs')
  #    puts method.size_opt_params # => 4
  def size_opt_params; end

  # Returns the size of arguments of the method.
  #    tobj = WIN32OLE_TYPE.new('Microsoft Excel 9.0 Object Library', 'Workbook')
  #    method = WIN32OLE_METHOD.new(tobj, 'SaveAs')
  #    puts method.size_params # => 11
  def size_params; end

  # Returns true if the method is public.
  #    tobj = WIN32OLE_TYPE.new('Microsoft Excel 9.0 Object Library', 'Workbooks')
  #    method = WIN32OLE_METHOD.new(tobj, 'Add')
  #    puts method.visible? # => true
  def visible?; end
end
