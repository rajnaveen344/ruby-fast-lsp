# frozen_string_literal: true

# +WIN32OLE+ objects represent OLE Automation object in Ruby.
#
# By using +WIN32OLE+, you can access OLE server like VBScript.
#
# Here is sample script.
#
#   require 'win32ole'
#
#   excel = WIN32OLE.new('Excel.Application')
#   excel.visible = true
#   workbook = excel.Workbooks.Add();
#   worksheet = workbook.Worksheets(1);
#   worksheet.Range("A1:D1").value = ["North","South","East","West"];
#   worksheet.Range("A2:B2").value = [5.2, 10];
#   worksheet.Range("C2").value = 8;
#   worksheet.Range("D2").value = 20;
#
#   range = worksheet.Range("A1:D2");
#   range.select
#   chart = workbook.Charts.Add;
#
#   workbook.saved = true;
#
#   excel.ActiveWorkbook.Close(0);
#   excel.Quit();
#
# Unfortunately, +WIN32OLE+ doesn't support the argument passed by
# reference directly.
# Instead, +WIN32OLE+ provides WIN32OLE::ARGV or WIN32OLE::Variant object.
# If you want to get the result value of argument passed by reference,
# you can use WIN32OLE::ARGV or WIN32OLE::Variant.
#
#   oleobj.method(arg1, arg2, refargv3)
#   puts WIN32OLE::ARGV[2]   # the value of refargv3 after called oleobj.method
#
# or
#
#   refargv3 = WIN32OLE::Variant.new(XXX,
#               WIN32OLE::VARIANT::VT_BYREF|WIN32OLE::VARIANT::VT_XXX)
#   oleobj.method(arg1, arg2, refargv3)
#   p refargv3.value # the value of refargv3 after called oleobj.method.
class WIN32OLE
  # After invoking OLE methods with reference arguments, you can access
  # the value of arguments by using ARGV.
  #
  # If the method of OLE(COM) server written by C#.NET is following:
  #
  #   void calcsum(int a, int b, out int c) {
  #       c = a + b;
  #   }
  #
  # then, the Ruby OLE(COM) client script to retrieve the value of
  # argument c after invoking calcsum method is following:
  #
  #   a = 10
  #   b = 20
  #   c = 0
  #   comserver.calcsum(a, b, c)
  #   p c # => 0
  #   p WIN32OLE::ARGV # => [10, 20, 30]
  #
  # You can use WIN32OLE::Variant object to retrieve the value of reference
  # arguments instead of referring WIN32OLE::ARGV.
  ARGV = _
  # ANSI code page. See WIN32OLE.codepage and WIN32OLE.codepage=.
  CP_ACP = _
  # 2
  CP_MACCP = _
  # OEM code page. See WIN32OLE.codepage and WIN32OLE.codepage=.
  CP_OEMCP = _
  # symbol code page. See WIN32OLE.codepage and WIN32OLE.codepage=.
  CP_SYMBOL = _
  # current thread ANSI code page. See WIN32OLE.codepage and
  # WIN32OLE.codepage=.
  CP_THREAD_ACP = _
  # UTF-7 code page. See WIN32OLE.codepage and WIN32OLE.codepage=.
  CP_UTF7 = _
  # UTF-8 code page. See WIN32OLE.codepage and WIN32OLE.codepage=.
  CP_UTF8 = _
  # default locale for the operating system. See WIN32OLE.locale
  # and WIN32OLE.locale=.
  LOCALE_SYSTEM_DEFAULT = _
  # default locale for the user or process. See WIN32OLE.locale
  # and WIN32OLE.locale=.
  LOCALE_USER_DEFAULT = _
  # Version string of WIN32OLE.
  VERSION = _

  # Returns current codepage.
  #    WIN32OLE.codepage # => WIN32OLE::CP_ACP
  def self.codepage; end

  # Sets current codepage.
  # The WIN32OLE.codepage is initialized according to
  # Encoding.default_internal.
  # If Encoding.default_internal is nil then WIN32OLE.codepage
  # is initialized according to Encoding.default_external.
  #
  #    WIN32OLE.codepage = WIN32OLE::CP_UTF8
  #    WIN32OLE.codepage = 65001
  def self.codepage=(codepage) end

  # Returns running OLE Automation object or WIN32OLE object from moniker.
  # 1st argument should be OLE program id or class id or moniker.
  #
  #    WIN32OLE.connect('Excel.Application') # => WIN32OLE object which represents running Excel.
  def self.connect(ole) end

  # Defines the constants of OLE Automation server as mod's constants.
  # The first argument is WIN32OLE object or type library name.
  # If 2nd argument is omitted, the default is WIN32OLE.
  # The first letter of Ruby's constant variable name is upper case,
  # so constant variable name of WIN32OLE object is capitalized.
  # For example, the 'xlTop' constant of Excel is changed to 'XlTop'
  # in WIN32OLE.
  # If the first letter of constant variable is not [A-Z], then
  # the constant is defined as CONSTANTS hash element.
  #
  #    module EXCEL_CONST
  #    end
  #    excel = WIN32OLE.new('Excel.Application')
  #    WIN32OLE.const_load(excel, EXCEL_CONST)
  #    puts EXCEL_CONST::XlTop # => -4160
  #    puts EXCEL_CONST::CONSTANTS['_xlDialogChartSourceData'] # => 541
  #
  #    WIN32OLE.const_load(excel)
  #    puts WIN32OLE::XlTop # => -4160
  #
  #    module MSO
  #    end
  #    WIN32OLE.const_load('Microsoft Office 9.0 Object Library', MSO)
  #    puts MSO::MsoLineSingle # => 1
  def self.const_load(ole, mod = WIN32OLE) end

  # Creates GUID.
  #    WIN32OLE.create_guid # => {1CB530F1-F6B1-404D-BCE6-1959BF91F4A8}
  def self.create_guid; end

  # Returns current locale id (lcid). The default locale is
  # WIN32OLE::LOCALE_SYSTEM_DEFAULT.
  #
  #    lcid = WIN32OLE.locale
  def self.locale; end

  # Sets current locale id (lcid).
  #
  #    WIN32OLE.locale = 1033 # set locale English(U.S)
  #    obj = WIN32OLE::Variant.new("$100,000", WIN32OLE::VARIANT::VT_CY)
  def self.locale=(lcid) end

  # Invokes Release method of Dispatch interface of WIN32OLE object.
  # You should not use this method because this method
  # exists only for debugging WIN32OLE.
  # The return value is reference counter of OLE object.
  def self.ole_free(win32_ole) end

  # Returns reference counter of Dispatch interface of WIN32OLE object.
  # You should not use this method because this method
  # exists only for debugging WIN32OLE.
  def self.ole_reference_count(win32_ole) end

  # Displays helpfile. The 1st argument specifies WIN32OLE::Type
  # object or WIN32OLE::Method object or helpfile.
  #
  #    excel = WIN32OLE.new('Excel.Application')
  #    typeobj = excel.ole_type
  #    WIN32OLE.ole_show_help(typeobj)
  def self.ole_show_help(obj, helpcontext = _) end

  # Returns a new WIN32OLE object(OLE Automation object).
  # The first argument server specifies OLE Automation server.
  # The first argument should be CLSID or PROGID.
  # If second argument host specified, then returns OLE Automation
  # object on host.
  # If :license keyword argument is provided,
  # IClassFactory2::CreateInstanceLic is used to create instance of
  # licensed server.
  #
  #     WIN32OLE.new('Excel.Application') # => Excel OLE Automation WIN32OLE object.
  #     WIN32OLE.new('{00024500-0000-0000-C000-000000000046}') # => Excel OLE Automation WIN32OLE object.
  def initialize(...) end

  # Returns the value of Collection specified by a1, a2,....
  #
  #    dict = WIN32OLE.new('Scripting.Dictionary')
  #    dict.add('ruby', 'Ruby')
  #    puts dict['ruby'] # => 'Ruby' (same as `puts dict.item('ruby')')
  #
  # Remark: You can not use this method to get the property.
  #    excel = WIN32OLE.new('Excel.Application')
  #    # puts excel['Visible']  This is error !!!
  #    puts excel.Visible # You should to use this style to get the property.
  def [](a1, a2, *args) end

  # Sets the value to WIN32OLE object specified by a1, a2, ...
  #
  #    dict = WIN32OLE.new('Scripting.Dictionary')
  #    dict.add('ruby', 'RUBY')
  #    dict['ruby'] = 'Ruby'
  #    puts dict['ruby'] # => 'Ruby'
  #
  # Remark: You can not use this method to set the property value.
  #
  #    excel = WIN32OLE.new('Excel.Application')
  #    # excel['Visible'] = true # This is error !!!
  #    excel.Visible = true # You should to use this style to set the property.
  def []=(a1, a2, *args, val) end

  # Runs the early binding method to get property.
  # The 1st argument specifies dispatch ID,
  # the 2nd argument specifies the array of arguments,
  # the 3rd argument specifies the array of the type of arguments.
  #
  #    excel = WIN32OLE.new('Excel.Application')
  #    puts excel._getproperty(558, [], []) # same effect as puts excel.visible
  def _getproperty(dispid, args, types) end

  # Runs the early binding method.
  # The 1st argument specifies dispatch ID,
  # the 2nd argument specifies the array of arguments,
  # the 3rd argument specifies the array of the type of arguments.
  #
  #    excel = WIN32OLE.new('Excel.Application')
  #    excel._invoke(302, [], []) #  same effect as excel.Quit
  def _invoke(dispid, args, types) end

  # Runs the early binding method to set property.
  # The 1st argument specifies dispatch ID,
  # the 2nd argument specifies the array of arguments,
  # the 3rd argument specifies the array of the type of arguments.
  #
  #    excel = WIN32OLE.new('Excel.Application')
  #    excel._setproperty(558, [true], [WIN32OLE::VARIANT::VT_BOOL]) # same effect as excel.visible = true
  def _setproperty(dispid, args, types) end

  # Iterates over each item of OLE collection which has IEnumVARIANT interface.
  #
  #    excel = WIN32OLE.new('Excel.Application')
  #    book = excel.workbooks.add
  #    sheets = book.worksheets(1)
  #    cells = sheets.cells("A1:A5")
  #    cells.each do |cell|
  #      cell.value = 10
  #    end
  def each; end

  # Runs OLE method.
  # The first argument specifies the method name of OLE Automation object.
  # The others specify argument of the <i>method</i>.
  # If you can not execute <i>method</i> directly, then use this method instead.
  #
  #   excel = WIN32OLE.new('Excel.Application')
  #   excel.invoke('Quit')  # => same as excel.Quit
  def invoke(method, *args) end

  # Calls WIN32OLE#invoke method.
  def method_missing(name, *args) end

  # Initialize WIN32OLE object(ActiveX Control) by calling
  # IPersistMemory::InitNew.
  #
  # Before calling OLE method, some kind of the ActiveX controls
  # created with MFC should be initialized by calling
  # IPersistXXX::InitNew.
  #
  # If and only if you received the exception "HRESULT error code:
  # 0x8000ffff catastrophic failure", try this method before
  # invoking any ole_method.
  #
  #    obj = WIN32OLE.new("ProgID_or_GUID_of_ActiveX_Control")
  #    obj.ole_activex_initialize
  #    obj.method(...)
  def ole_activex_initialize; end

  # invokes Release method of Dispatch interface of WIN32OLE object.
  # Usually, you do not need to call this method because Release method
  # called automatically when WIN32OLE object garbaged.
  def ole_free; end

  # Returns the array of WIN32OLE::Method object .
  # The element of the array is property (settable) of WIN32OLE object.
  #
  #    excel = WIN32OLE.new('Excel.Application')
  #    properties = excel.ole_func_methods
  def ole_func_methods; end

  # Returns the array of WIN32OLE::Method object .
  # The element of the array is property (gettable) of WIN32OLE object.
  #
  #    excel = WIN32OLE.new('Excel.Application')
  #    properties = excel.ole_get_methods
  def ole_get_methods; end

  # Returns WIN32OLE::Method object corresponding with method
  # specified by 1st argument.
  #
  #    excel = WIN32OLE.new('Excel.Application')
  #    method = excel.ole_method_help('Quit')
  def ole_method(p1) end
  alias ole_method_help ole_method

  # Returns the array of WIN32OLE::Method object.
  # The element is OLE method of WIN32OLE object.
  #
  #    excel = WIN32OLE.new('Excel.Application')
  #    methods = excel.ole_methods
  def ole_methods; end

  # Returns the array of WIN32OLE::Method object .
  # The element of the array is property (settable) of WIN32OLE object.
  #
  #    excel = WIN32OLE.new('Excel.Application')
  #    properties = excel.ole_put_methods
  def ole_put_methods; end

  # Returns WIN32OLE object for a specific dispatch or dual
  # interface specified by iid.
  #
  #     ie = WIN32OLE.new('InternetExplorer.Application')
  #     ie_web_app = ie.ole_query_interface('{0002DF05-0000-0000-C000-000000000046}') # => WIN32OLE object for dispinterface IWebBrowserApp
  def ole_query_interface(iid) end

  # Returns true when OLE object has OLE method, otherwise returns false.
  #
  #     ie = WIN32OLE.new('InternetExplorer.Application')
  #     ie.ole_respond_to?("gohome") => true
  def ole_respond_to?(method) end

  # Returns WIN32OLE::Type object.
  #
  #    excel = WIN32OLE.new('Excel.Application')
  #    tobj = excel.ole_type
  def ole_type; end
  alias ole_obj_help ole_type

  # Returns the WIN32OLE::TypeLib object. The object represents the
  # type library which contains the WIN32OLE object.
  #
  #    excel = WIN32OLE.new('Excel.Application')
  #    tlib = excel.ole_typelib
  #    puts tlib.name  # -> 'Microsoft Excel 9.0 Object Library'
  def ole_typelib; end

  # Sets property of OLE object.
  # When you want to set property with argument, you can use this method.
  #
  #    excel = WIN32OLE.new('Excel.Application')
  #    excel.Visible = true
  #    book = excel.workbooks.add
  #    sheet = book.worksheets(1)
  #    sheet.setproperty('Cells', 1, 2, 10) # => The B1 cell value is 10.
  def setproperty(property, *args) end

  # +WIN32OLE::Event+ objects controls OLE event.
  class Event
    # Translates and dispatches Windows message.
    def self.message_loop; end

    # Returns OLE event object.
    # The first argument specifies WIN32OLE object.
    # The second argument specifies OLE event name.
    #    ie = WIN32OLE.new('InternetExplorer.Application')
    #    ev = WIN32OLE::Event.new(ie, 'DWebBrowserEvents')
    def initialize(ole, event) end

    # returns handler object.
    def handler; end

    # sets event handler object. If handler object has onXXX
    # method according to XXX event, then onXXX method is called
    # when XXX event occurs.
    #
    # If handler object has method_missing and there is no
    # method according to the event, then method_missing
    # called and 1-st argument is event name.
    #
    # If handler object has onXXX method and there is block
    # defined by <code>on_event('XXX'){}</code>,
    # then block is executed but handler object method is not called
    # when XXX event occurs.
    #
    #     class Handler
    #       def onStatusTextChange(text)
    #         puts "StatusTextChanged"
    #       end
    #       def onPropertyChange(prop)
    #         puts "PropertyChanged"
    #       end
    #       def method_missing(ev, *arg)
    #         puts "other event #{ev}"
    #       end
    #     end
    #
    #     handler = Handler.new
    #     ie = WIN32OLE.new('InternetExplorer.Application')
    #     ev = WIN32OLE::Event.new(ie)
    #     ev.on_event("StatusTextChange") {|*args|
    #       puts "this block executed."
    #       puts "handler.onStatusTextChange method is not called."
    #     }
    #     ev.handler = handler
    def handler=; end

    # removes the callback of event.
    #
    #   ie = WIN32OLE.new('InternetExplorer.Application')
    #   ev = WIN32OLE::Event.new(ie)
    #   ev.on_event('BeforeNavigate2') {|*args|
    #     args.last[6] = true
    #   }
    #   # ...
    #   ev.off_event('BeforeNavigate2')
    #   # ...
    def off_event(*event) end

    # Defines the callback event.
    # If argument is omitted, this method defines the callback of all events.
    # If you want to modify reference argument in callback, return hash in
    # callback. If you want to return value to OLE server as result of callback
    # use `return' or :return.
    #
    #   ie = WIN32OLE.new('InternetExplorer.Application')
    #   ev = WIN32OLE::Event.new(ie)
    #   ev.on_event("NavigateComplete") {|url| puts url}
    #   ev.on_event() {|ev, *args| puts "#{ev} fired"}
    #
    #   ev.on_event("BeforeNavigate2") {|*args|
    #     # ...
    #     # set true to BeforeNavigate reference argument `Cancel'.
    #     # Cancel is 7-th argument of BeforeNavigate,
    #     # so you can use 6 as key of hash instead of 'Cancel'.
    #     # The argument is counted from 0.
    #     # The hash key of 0 means first argument.)
    #     {:Cancel => true}  # or {'Cancel' => true} or {6 => true}
    #   }
    #
    #   ev.on_event(event_name) {|*args|
    #     {:return => 1, :xxx => yyy}
    #   }
    def on_event(*event) end

    # Defines the callback of event.
    # If you want modify argument in callback,
    # you could use this method instead of WIN32OLE::Event#on_event.
    #
    #   ie = WIN32OLE.new('InternetExplorer.Application')
    #   ev = WIN32OLE::Event.new(ie)
    #   ev.on_event_with_outargs('BeforeNavigate2') {|*args|
    #     args.last[6] = true
    #   }
    def on_event_with_outargs(*event) end

    # disconnects OLE server. If this method called, then the WIN32OLE::Event object
    # does not receive the OLE server event any more.
    # This method is trial implementation.
    #
    #     ie = WIN32OLE.new('InternetExplorer.Application')
    #     ev = WIN32OLE::Event.new(ie)
    #     ev.on_event() { something }
    #     # ...
    #     ev.unadvise
    def unadvise; end
  end

  # +WIN32OLE::Method+ objects represent OLE method information.
  class Method
    # Returns a new WIN32OLE::Method object which represents the information
    # about OLE method.
    # The first argument <i>ole_type</i> specifies WIN32OLE::Type object.
    # The second argument <i>method</i> specifies OLE method name defined OLE class
    # which represents WIN32OLE::Type object.
    #
    #    tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'Workbook')
    #    method = WIN32OLE::Method.new(tobj, 'SaveAs')
    def initialize(ole_type, method) end

    # Returns dispatch ID.
    #    tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'Workbooks')
    #    method = WIN32OLE::Method.new(tobj, 'Add')
    #    puts method.dispid # => 181
    def dispid; end

    # Returns true if the method is event.
    #    tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'Workbook')
    #    method = WIN32OLE::Method.new(tobj, 'SheetActivate')
    #    puts method.event? # => true
    def event?; end

    # Returns event interface name if the method is event.
    #   tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'Workbook')
    #   method = WIN32OLE::Method.new(tobj, 'SheetActivate')
    #   puts method.event_interface # =>  WorkbookEvents
    def event_interface; end

    # Returns help context.
    #    tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'Workbooks')
    #    method = WIN32OLE::Method.new(tobj, 'Add')
    #    puts method.helpcontext # => 65717
    def helpcontext; end

    # Returns help file. If help file is not found, then
    # the method returns nil.
    #    tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'Workbooks')
    #    method = WIN32OLE::Method.new(tobj, 'Add')
    #    puts method.helpfile # => C:\...\VBAXL9.CHM
    def helpfile; end

    # Returns help string of OLE method. If the help string is not found,
    # then the method returns nil.
    #    tobj = WIN32OLE::Type.new('Microsoft Internet Controls', 'IWebBrowser')
    #    method = WIN32OLE::Method.new(tobj, 'Navigate')
    #    puts method.helpstring # => Navigates to a URL or file.
    def helpstring; end

    # Returns the method name with class name.
    def inspect; end

    # Returns the method invoke kind.
    #   tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'Workbooks')
    #   method = WIN32OLE::Method.new(tobj, 'Add')
    #   puts method.invkind # => 1
    def invkind; end

    # Returns the method kind string. The string is "UNKNOWN" or "PROPERTY"
    # or "PROPERTY" or "PROPERTYGET" or "PROPERTYPUT" or "PROPERTYPPUTREF"
    # or "FUNC".
    #    tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'Workbooks')
    #    method = WIN32OLE::Method.new(tobj, 'Add')
    #    puts method.invoke_kind # => "FUNC"
    def invoke_kind; end

    # Returns the name of the method.
    #
    #    tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'Workbook')
    #    method = WIN32OLE::Method.new(tobj, 'SaveAs')
    #    puts method.name # => SaveAs
    def name; end
    alias to_s name

    # Returns the offset ov VTBL.
    #    tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'Workbooks')
    #    method = WIN32OLE::Method.new(tobj, 'Add')
    #    puts method.offset_vtbl # => 40
    def offset_vtbl; end

    # returns array of WIN32OLE::Param object corresponding with method parameters.
    #    tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'Workbook')
    #    method = WIN32OLE::Method.new(tobj, 'SaveAs')
    #    p method.params # => [Filename, FileFormat, Password, WriteResPassword,
    #                          ReadOnlyRecommended, CreateBackup, AccessMode,
    #                          ConflictResolution, AddToMru, TextCodepage,
    #                          TextVisualLayout]
    def params; end

    # Returns string of return value type of method.
    #    tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'Workbooks')
    #    method = WIN32OLE::Method.new(tobj, 'Add')
    #    puts method.return_type # => Workbook
    def return_type; end

    # Returns detail information of return value type of method.
    # The information is array.
    #    tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'Workbooks')
    #    method = WIN32OLE::Method.new(tobj, 'Add')
    #    p method.return_type_detail # => ["PTR", "USERDEFINED", "Workbook"]
    def return_type_detail; end

    # Returns number of return value type of method.
    #    tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'Workbooks')
    #    method = WIN32OLE::Method.new(tobj, 'Add')
    #    puts method.return_vtype # => 26
    def return_vtype; end

    # Returns the size of optional parameters.
    #    tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'Workbook')
    #    method = WIN32OLE::Method.new(tobj, 'SaveAs')
    #    puts method.size_opt_params # => 4
    def size_opt_params; end

    # Returns the size of arguments of the method.
    #    tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'Workbook')
    #    method = WIN32OLE::Method.new(tobj, 'SaveAs')
    #    puts method.size_params # => 11
    def size_params; end

    # Returns true if the method is public.
    #    tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'Workbooks')
    #    method = WIN32OLE::Method.new(tobj, 'Add')
    #    puts method.visible? # => true
    def visible?; end
  end

  # +WIN32OLE::Param+ objects represent param information of
  # the OLE method.
  class Param
    # Returns WIN32OLE::Param object which represents OLE parameter information.
    # 1st argument should be WIN32OLE::Method object.
    # 2nd argument `n' is n-th parameter of the method specified by 1st argument.
    #
    #    tobj = WIN32OLE::Type.new('Microsoft Scripting Runtime', 'IFileSystem')
    #    method = WIN32OLE::Method.new(tobj, 'CreateTextFile')
    #    param = WIN32OLE::Param.new(method, 2) # => #<WIN32OLE::Param:Overwrite=true>
    def initialize(method, n) end

    # Returns default value. If the default value does not exist,
    # this method returns nil.
    #    tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'Workbook')
    #    method = WIN32OLE::Method.new(tobj, 'SaveAs')
    #    method.params.each do |param|
    #      if param.default
    #        puts "#{param.name} (= #{param.default})"
    #      else
    #        puts "#{param}"
    #      end
    #    end
    #
    # The above script result is following:
    #     Filename
    #     FileFormat
    #     Password
    #     WriteResPassword
    #     ReadOnlyRecommended
    #     CreateBackup
    #     AccessMode (= 1)
    #     ConflictResolution
    #     AddToMru
    #     TextCodepage
    #     TextVisualLayout
    def default; end

    # Returns true if the parameter is input.
    #    tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'Workbook')
    #    method = WIN32OLE::Method.new(tobj, 'SaveAs')
    #    param1 = method.params[0]
    #    puts param1.input? # => true
    def input?; end

    # Returns the parameter name with class name. If the parameter has default value,
    # then returns name=value string with class name.
    def inspect; end

    # Returns name.
    #    tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'Workbook')
    #    method = WIN32OLE::Method.new(tobj, 'SaveAs')
    #    param1 = method.params[0]
    #    puts param1.name # => Filename
    def name; end
    alias to_s name

    # Returns OLE type of WIN32OLE::Param object(parameter of OLE method).
    #    tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'Workbook')
    #    method = WIN32OLE::Method.new(tobj, 'SaveAs')
    #    param1 = method.params[0]
    #    puts param1.ole_type # => VARIANT
    def ole_type; end

    # Returns detail information of type of argument.
    #    tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'IWorksheetFunction')
    #    method = WIN32OLE::Method.new(tobj, 'SumIf')
    #    param1 = method.params[0]
    #    p param1.ole_type_detail # => ["PTR", "USERDEFINED", "Range"]
    def ole_type_detail; end

    # Returns true if argument is optional.
    #    tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'Workbook')
    #    method = WIN32OLE::Method.new(tobj, 'SaveAs')
    #    param1 = method.params[0]
    #    puts "#{param1.name} #{param1.optional?}" # => Filename true
    def optional?; end

    # Returns true if argument is output.
    #    tobj = WIN32OLE::Type.new('Microsoft Internet Controls', 'DWebBrowserEvents')
    #    method = WIN32OLE::Method.new(tobj, 'NewWindow')
    #    method.params.each do |param|
    #      puts "#{param.name} #{param.output?}"
    #    end
    #
    # The result of above script is following:
    #   URL false
    #   Flags false
    #   TargetFrameName false
    #   PostData false
    #   Headers false
    #   Processed true
    def output?; end

    # Returns true if argument is return value.
    #    tobj = WIN32OLE::Type.new('DirectX 7 for Visual Basic Type Library',
    #                              'DirectPlayLobbyConnection')
    #    method = WIN32OLE::Method.new(tobj, 'GetPlayerShortName')
    #    param = method.params[0]
    #    puts "#{param.name} #{param.retval?}"  # => name true
    def retval?; end
  end

  # Raised when OLE query failed.
  class QueryInterfaceError < RuntimeError
  end

  # +WIN32OLE::Record+ objects represents VT_RECORD OLE variant.
  # Win32OLE returns WIN32OLE::Record object if the result value of invoking
  # OLE methods.
  #
  # If COM server in VB.NET ComServer project is the following:
  #
  #   Imports System.Runtime.InteropServices
  #   Public Class ComClass
  #       Public Structure Book
  #           <MarshalAs(UnmanagedType.BStr)> _
  #           Public title As String
  #           Public cost As Integer
  #       End Structure
  #       Public Function getBook() As Book
  #           Dim book As New Book
  #           book.title = "The Ruby Book"
  #           book.cost = 20
  #           Return book
  #       End Function
  #   End Class
  #
  # then, you can retrieve getBook return value from the following
  # Ruby script:
  #
  #   require 'win32ole'
  #   obj = WIN32OLE.new('ComServer.ComClass')
  #   book = obj.getBook
  #   book.class # => WIN32OLE::Record
  #   book.title # => "The Ruby Book"
  #   book.cost  # => 20
  class Record
    # Returns WIN32OLE::Record object. The first argument is struct name (String
    # or Symbol).
    # The second parameter obj should be WIN32OLE object or WIN32OLE::TypeLib object.
    # If COM server in VB.NET ComServer project is the following:
    #
    #   Imports System.Runtime.InteropServices
    #   Public Class ComClass
    #       Public Structure Book
    #           <MarshalAs(UnmanagedType.BStr)> _
    #           Public title As String
    #           Public cost As Integer
    #       End Structure
    #   End Class
    #
    # then, you can create WIN32OLE::Record object is as following:
    #
    #   require 'win32ole'
    #   obj = WIN32OLE.new('ComServer.ComClass')
    #   book1 = WIN32OLE::Record.new('Book', obj) # => WIN32OLE::Record object
    #   tlib = obj.ole_typelib
    #   book2 = WIN32OLE::Record.new('Book', tlib) # => WIN32OLE::Record object
    def initialize(typename, obj) end

    # Returns the OLE struct name and member name and the value of member
    #
    # If COM server in VB.NET ComServer project is the following:
    #
    #    Imports System.Runtime.InteropServices
    #    Public Class ComClass
    #        <MarshalAs(UnmanagedType.BStr)> _
    #        Public title As String
    #        Public cost As Integer
    #    End Class
    #
    # then
    #
    #    srver = WIN32OLE.new('ComServer.ComClass')
    #    obj = WIN32OLE::Record.new('Book', server)
    #    obj.inspect # => <WIN32OLE::Record(ComClass) {"title" => nil, "cost" => nil}>
    def inspect; end

    # Returns value specified by the member name of VT_RECORD OLE variable.
    # Or sets value specified by the member name of VT_RECORD OLE variable.
    # If the member name is not correct, KeyError exception is raised.
    #
    # If COM server in VB.NET ComServer project is the following:
    #
    #    Imports System.Runtime.InteropServices
    #    Public Class ComClass
    #        Public Structure Book
    #            <MarshalAs(UnmanagedType.BStr)> _
    #            Public title As String
    #            Public cost As Integer
    #        End Structure
    #    End Class
    #
    # Then getting/setting value from Ruby is as the following:
    #
    #    obj = WIN32OLE.new('ComServer.ComClass')
    #    book = WIN32OLE::Record.new('Book', obj)
    #    book.title # => nil ( book.method_missing(:title) is invoked. )
    #    book.title = "Ruby" # ( book.method_missing(:title=, "Ruby") is invoked. )
    def method_missing(name) end

    # Returns value specified by the member name of VT_RECORD OLE object.
    # If the member name is not correct, KeyError exception is raised.
    # If you can't access member variable of VT_RECORD OLE object directly,
    # use this method.
    #
    # If COM server in VB.NET ComServer project is the following:
    #
    #    Imports System.Runtime.InteropServices
    #    Public Class ComClass
    #        Public Structure ComObject
    #            Public object_id As Ineger
    #        End Structure
    #    End Class
    #
    # and Ruby Object class has title attribute:
    #
    # then accessing object_id of ComObject from Ruby is as the following:
    #
    #    srver = WIN32OLE.new('ComServer.ComClass')
    #    obj = WIN32OLE::Record.new('ComObject', server)
    #    # obj.object_id returns Ruby Object#object_id
    #    obj.ole_instance_variable_get(:object_id) # => nil
    def ole_instance_variable_get(name) end

    # Sets value specified by the member name of VT_RECORD OLE object.
    # If the member name is not correct, KeyError exception is raised.
    # If you can't set value of member of VT_RECORD OLE object directly,
    # use this method.
    #
    # If COM server in VB.NET ComServer project is the following:
    #
    #    Imports System.Runtime.InteropServices
    #    Public Class ComClass
    #        <MarshalAs(UnmanagedType.BStr)> _
    #        Public title As String
    #        Public cost As Integer
    #    End Class
    #
    # then setting value of the `title' member is as following:
    #
    #    srver = WIN32OLE.new('ComServer.ComClass')
    #    obj = WIN32OLE::Record.new('Book', server)
    #    obj.ole_instance_variable_set(:title, "The Ruby Book")
    def ole_instance_variable_set(name, val) end

    # Returns Ruby Hash object which represents VT_RECORD variable.
    # The keys of Hash object are member names of VT_RECORD OLE variable and
    # the values of Hash object are values of VT_RECORD OLE variable.
    #
    # If COM server in VB.NET ComServer project is the following:
    #
    #    Imports System.Runtime.InteropServices
    #    Public Class ComClass
    #        Public Structure Book
    #            <MarshalAs(UnmanagedType.BStr)> _
    #            Public title As String
    #            Public cost As Integer
    #        End Structure
    #        Public Function getBook() As Book
    #            Dim book As New Book
    #            book.title = "The Ruby Book"
    #            book.cost = 20
    #            Return book
    #        End Function
    #    End Class
    #
    # then, the result of WIN32OLE::Record#to_h is the following:
    #
    #    require 'win32ole'
    #    obj = WIN32OLE.new('ComServer.ComClass')
    #    book = obj.getBook
    #    book.to_h # => {"title"=>"The Ruby Book", "cost"=>20}
    def to_h; end

    # Returns the type name of VT_RECORD OLE variable.
    #
    # If COM server in VB.NET ComServer project is the following:
    #
    #    Imports System.Runtime.InteropServices
    #    Public Class ComClass
    #        Public Structure Book
    #            <MarshalAs(UnmanagedType.BStr)> _
    #            Public title As String
    #            Public cost As Integer
    #        End Structure
    #        Public Function getBook() As Book
    #            Dim book As New Book
    #            book.title = "The Ruby Book"
    #            book.cost = 20
    #            Return book
    #        End Function
    #    End Class
    #
    # then, the result of WIN32OLE::Record#typename is the following:
    #
    #    require 'win32ole'
    #    obj = WIN32OLE.new('ComServer.ComClass')
    #    book = obj.getBook
    #    book.typename # => "Book"
    def typename; end
  end

  # Raised when OLE processing failed.
  #
  # EX:
  #
  #   obj = WIN32OLE.new("NonExistProgID")
  #
  # raises the exception:
  #
  #   WIN32OLE::RuntimeError: unknown OLE server: `NonExistProgID'
  #       HRESULT error code:0x800401f3
  #         Invalid class string
  class RuntimeError < RuntimeError
  end

  # +WIN32OLE::Type+ objects represent OLE type library information.
  class Type
    # Returns array of WIN32OLE::Type objects defined by the <i>typelib</i> type library.
    #
    # This method will be OBSOLETE.
    # Use <code>WIN32OLE::TypeLib.new(typelib).ole_classes</code> instead.
    def self.ole_classes(typelib) end

    # Returns array of ProgID.
    def self.progids; end

    # Returns array of type libraries.
    #
    # This method will be OBSOLETE.
    # Use <code>WIN32OLE::TypeLib.typelibs.collect{|t| t.name}</code> instead.
    def self.typelibs; end

    # Returns a new WIN32OLE::Type object.
    # The first argument <i>typelib</i> specifies OLE type library name.
    # The second argument specifies OLE class name.
    #
    #     WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'Application')
    #         # => WIN32OLE::Type object of Application class of Excel.
    def initialize(typelib, ole_class) end

    # Returns the array of WIN32OLE::Type object which is implemented by the WIN32OLE::Type
    # object and having IMPLTYPEFLAG_FSOURCE and IMPLTYPEFLAG_FDEFAULT.
    #    tobj = WIN32OLE::Type.new('Microsoft Internet Controls', "InternetExplorer")
    #    p tobj.default_event_sources  # => [#<WIN32OLE::Type:DWebBrowserEvents2>]
    def default_event_sources; end

    # Returns the array of WIN32OLE::Type object which is implemented by the WIN32OLE::Type
    # object and having IMPLTYPEFLAG_FDEFAULT.
    #    tobj = WIN32OLE::Type.new('Microsoft Internet Controls', "InternetExplorer")
    #    p tobj.default_ole_types
    #    # => [#<WIN32OLE::Type:IWebBrowser2>, #<WIN32OLE::Type:DWebBrowserEvents2>]
    def default_ole_types; end

    # Returns GUID.
    #   tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'Application')
    #   puts tobj.guid  # => {00024500-0000-0000-C000-000000000046}
    def guid; end

    # Returns helpcontext. If helpcontext is not found, then returns nil.
    #    tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'Worksheet')
    #    puts tobj.helpfile # => 131185
    def helpcontext; end

    # Returns helpfile path. If helpfile is not found, then returns nil.
    #    tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'Worksheet')
    #    puts tobj.helpfile # => C:\...\VBAXL9.CHM
    def helpfile; end

    # Returns help string.
    #   tobj = WIN32OLE::Type.new('Microsoft Internet Controls', 'IWebBrowser')
    #   puts tobj.helpstring # => Web Browser interface
    def helpstring; end

    # Returns the array of WIN32OLE::Type object which is implemented by the WIN32OLE::Type
    # object.
    #    tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'Worksheet')
    #    p tobj.implemented_ole_types # => [_Worksheet, DocEvents]
    def implemented_ole_types; end

    # Returns the type name with class name.
    #
    #    ie = WIN32OLE.new('InternetExplorer.Application')
    #    ie.ole_type.inspect => #<WIN32OLE::Type:IWebBrowser2>
    def inspect; end

    # Returns major version.
    #    tobj = WIN32OLE::Type.new('Microsoft Word 10.0 Object Library', 'Documents')
    #    puts tobj.major_version # => 8
    def major_version; end

    # Returns minor version.
    #    tobj = WIN32OLE::Type.new('Microsoft Word 10.0 Object Library', 'Documents')
    #    puts tobj.minor_version # => 2
    def minor_version; end

    # Returns OLE type name.
    #    tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'Application')
    #    puts tobj.name  # => Application
    def name; end
    alias to_s name

    # Returns array of WIN32OLE::Method objects which represent OLE method defined in
    # OLE type library.
    #   tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'Worksheet')
    #   methods = tobj.ole_methods.collect{|m|
    #     m.name
    #   }
    #   # => ['Activate', 'Copy', 'Delete',....]
    def ole_methods; end

    # returns type of OLE class.
    #   tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'Application')
    #   puts tobj.ole_type  # => Class
    def ole_type; end

    # Returns the WIN32OLE::TypeLib object which is including the WIN32OLE::Type
    # object. If it is not found, then returns nil.
    #    tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'Worksheet')
    #    puts tobj.ole_typelib # => 'Microsoft Excel 9.0 Object Library'
    def ole_typelib; end

    # Returns ProgID if it exists. If not found, then returns nil.
    #    tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'Application')
    #    puts tobj.progid  # =>   Excel.Application.9
    def progid; end

    # Returns the array of WIN32OLE::Type object which is implemented by the WIN32OLE::Type
    # object and having IMPLTYPEFLAG_FSOURCE.
    #    tobj = WIN32OLE::Type.new('Microsoft Internet Controls', "InternetExplorer")
    #    p tobj.source_ole_types
    #    # => [#<WIN32OLE::Type:DWebBrowserEvents2>, #<WIN32OLE::Type:DWebBrowserEvents>]
    def source_ole_types; end

    # Returns source class when the OLE class is 'Alias'.
    #    tobj =  WIN32OLE::Type.new('Microsoft Office 9.0 Object Library', 'MsoRGBType')
    #    puts tobj.src_type # => I4
    def src_type; end

    # Returns number which represents type.
    #   tobj = WIN32OLE::Type.new('Microsoft Word 10.0 Object Library', 'Documents')
    #   puts tobj.typekind # => 4
    def typekind; end

    # Returns array of WIN32OLE::Variable objects which represent variables
    # defined in OLE class.
    #    tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'XlSheetType')
    #    vars = tobj.variables
    #    vars.each do |v|
    #      puts "#{v.name} = #{v.value}"
    #    end
    #
    #    The result of above sample script is follows:
    #      xlChart = -4109
    #      xlDialogSheet = -4116
    #      xlExcel4IntlMacroSheet = 4
    #      xlExcel4MacroSheet = 3
    #      xlWorksheet = -4167
    def variables; end

    # Returns true if the OLE class is public.
    #   tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'Application')
    #   puts tobj.visible  # => true
    def visible?; end
  end

  # +WIN32OLE::TypeLib+ objects represent OLE tyblib information.
  class TypeLib
    #    typelibs
    #
    # Returns the array of WIN32OLE::TypeLib object.
    #
    #    tlibs = WIN32OLE::TypeLib.typelibs
    def self.typelibs; end

    # Returns a new WIN32OLE::TypeLib object.
    #
    # The first argument <i>typelib</i>  specifies OLE type library name or GUID or
    # OLE library file.
    # The second argument is major version or version of the type library.
    # The third argument is minor version.
    # The second argument and third argument are optional.
    # If the first argument is type library name, then the second and third argument
    # are ignored.
    #
    #     tlib1 = WIN32OLE::TypeLib.new('Microsoft Excel 9.0 Object Library')
    #     tlib2 = WIN32OLE::TypeLib.new('{00020813-0000-0000-C000-000000000046}')
    #     tlib3 = WIN32OLE::TypeLib.new('{00020813-0000-0000-C000-000000000046}', 1.3)
    #     tlib4 = WIN32OLE::TypeLib.new('{00020813-0000-0000-C000-000000000046}', 1, 3)
    #     tlib5 = WIN32OLE::TypeLib.new("C:\\WINNT\\SYSTEM32\\SHELL32.DLL")
    #     puts tlib1.name  # -> 'Microsoft Excel 9.0 Object Library'
    #     puts tlib2.name  # -> 'Microsoft Excel 9.0 Object Library'
    #     puts tlib3.name  # -> 'Microsoft Excel 9.0 Object Library'
    #     puts tlib4.name  # -> 'Microsoft Excel 9.0 Object Library'
    #     puts tlib5.name  # -> 'Microsoft Shell Controls And Automation'
    def initialize(*args) end

    # Returns guid string which specifies type library.
    #
    #    tlib = WIN32OLE::TypeLib.new('Microsoft Excel 9.0 Object Library')
    #    guid = tlib.guid # -> '{00020813-0000-0000-C000-000000000046}'
    def guid; end

    # Returns the type library name with class name.
    #
    #    tlib = WIN32OLE::TypeLib.new('Microsoft Excel 9.0 Object Library')
    #    tlib.inspect # => "<#WIN32OLE::TypeLib:Microsoft Excel 9.0 Object Library>"
    def inspect; end

    # Returns library name.
    # If the method fails to access library name, WIN32OLE::RuntimeError is raised.
    #
    #    tlib = WIN32OLE::TypeLib.new('Microsoft Excel 9.0 Object Library')
    #    tlib.library_name # => Excel
    def library_name; end

    # Returns the type library major version.
    #
    #    tlib = WIN32OLE::TypeLib.new('Microsoft Excel 9.0 Object Library')
    #    puts tlib.major_version # -> 1
    def major_version; end

    # Returns the type library minor version.
    #
    #    tlib = WIN32OLE::TypeLib.new('Microsoft Excel 9.0 Object Library')
    #    puts tlib.minor_version # -> 3
    def minor_version; end

    # Returns the type library name.
    #
    #    tlib = WIN32OLE::TypeLib.new('Microsoft Excel 9.0 Object Library')
    #    name = tlib.name # -> 'Microsoft Excel 9.0 Object Library'
    def name; end
    alias to_s name

    # Returns the type library file path.
    #
    #    tlib = WIN32OLE::TypeLib.new('Microsoft Excel 9.0 Object Library')
    #    classes = tlib.ole_types.collect{|k| k.name} # -> ['AddIn', 'AddIns' ...]
    def ole_types; end
    alias ole_classes ole_types

    # Returns the type library file path.
    #
    #    tlib = WIN32OLE::TypeLib.new('Microsoft Excel 9.0 Object Library')
    #    puts tlib.path #-> 'C:\...\EXCEL9.OLB'
    def path; end

    # Returns the type library version.
    #
    #    tlib = WIN32OLE::TypeLib.new('Microsoft Excel 9.0 Object Library')
    #    puts tlib.version #-> "1.3"
    def version; end

    # Returns true if the type library information is not hidden.
    # If wLibFlags of TLIBATTR is 0 or LIBFLAG_FRESTRICTED or LIBFLAG_FHIDDEN,
    # the method returns false, otherwise, returns true.
    # If the method fails to access the TLIBATTR information, then
    # WIN32OLE::RuntimeError is raised.
    #
    #    tlib = WIN32OLE::TypeLib.new('Microsoft Excel 9.0 Object Library')
    #    tlib.visible? # => true
    def visible?; end
  end

  # +WIN32OLE::Variable+ objects represent OLE variable information.
  class Variable
    # Returns the OLE variable name and the value with class name.
    def inspect; end

    # Returns the name of variable.
    #
    #    tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'XlSheetType')
    #    variables = tobj.variables
    #    variables.each do |variable|
    #      puts "#{variable.name}"
    #    end
    #
    #    The result of above script is following:
    #      xlChart
    #      xlDialogSheet
    #      xlExcel4IntlMacroSheet
    #      xlExcel4MacroSheet
    #      xlWorksheet
    def name; end
    alias to_s name

    # Returns OLE type string.
    #
    #   tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'XlSheetType')
    #   variables = tobj.variables
    #   variables.each do |variable|
    #     puts "#{variable.ole_type} #{variable.name}"
    #   end
    #
    #   The result of above script is following:
    #     INT xlChart
    #     INT xlDialogSheet
    #     INT xlExcel4IntlMacroSheet
    #     INT xlExcel4MacroSheet
    #     INT xlWorksheet
    def ole_type; end

    # Returns detail information of type. The information is array of type.
    #
    #    tobj = WIN32OLE::Type.new('DirectX 7 for Visual Basic Type Library', 'D3DCLIPSTATUS')
    #    variable = tobj.variables.find {|variable| variable.name == 'lFlags'}
    #    tdetail  = variable.ole_type_detail
    #    p tdetail # => ["USERDEFINED", "CONST_D3DCLIPSTATUSFLAGS"]
    def ole_type_detail; end

    # Returns value if value is exists. If the value does not exist,
    # this method returns nil.
    #
    #    tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'XlSheetType')
    #    variables = tobj.variables
    #    variables.each do |variable|
    #      puts "#{variable.name} #{variable.value}"
    #    end
    #
    #    The result of above script is following:
    #      xlChart = -4109
    #      xlDialogSheet = -4116
    #      xlExcel4IntlMacroSheet = 4
    #      xlExcel4MacroSheet = 3
    #      xlWorksheet = -4167
    def value; end

    # Returns variable kind string.
    #
    #    tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'XlSheetType')
    #    variables = tobj.variables
    #    variables.each do |variable|
    #      puts "#{variable.name} #{variable.variable_kind}"
    #    end
    #
    #    The result of above script is following:
    #      xlChart CONSTANT
    #      xlDialogSheet CONSTANT
    #      xlExcel4IntlMacroSheet CONSTANT
    #      xlExcel4MacroSheet CONSTANT
    #      xlWorksheet CONSTANT
    def variable_kind; end

    # Returns the number which represents variable kind.
    #   tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'XlSheetType')
    #   variables = tobj.variables
    #   variables.each do |variable|
    #     puts "#{variable.name} #{variable.varkind}"
    #   end
    #
    #   The result of above script is following:
    #      xlChart 2
    #      xlDialogSheet 2
    #      xlExcel4IntlMacroSheet 2
    #      xlExcel4MacroSheet 2
    #      xlWorksheet 2
    def varkind; end

    # Returns true if the variable is public.
    #
    #    tobj = WIN32OLE::Type.new('Microsoft Excel 9.0 Object Library', 'XlSheetType')
    #    variables = tobj.variables
    #    variables.each do |variable|
    #      puts "#{variable.name} #{variable.visible?}"
    #    end
    #
    #    The result of above script is following:
    #      xlChart true
    #      xlDialogSheet true
    #      xlExcel4IntlMacroSheet true
    #      xlExcel4MacroSheet true
    #      xlWorksheet true
    def visible?; end
  end

  # +WIN32OLE::Variant+ objects represents OLE variant.
  #
  # Win32OLE converts Ruby object into OLE variant automatically when
  # invoking OLE methods. If OLE method requires the argument which is
  # different from the variant by automatic conversion of Win32OLE, you
  # can convert the specified variant type by using WIN32OLE::Variant class.
  #
  #   param = WIN32OLE::Variant.new(10, WIN32OLE::VARIANT::VT_R4)
  #   oleobj.method(param)
  #
  # WIN32OLE::Variant does not support VT_RECORD variant. Use WIN32OLE::Record
  # class instead of WIN32OLE::Variant if the VT_RECORD variant is needed.
  class Variant
    # represents VT_EMPTY OLE object.
    Empty = _
    # represents VT_ERROR variant with DISP_E_PARAMNOTFOUND.
    # This constants is used for not specified parameter.
    #
    #  fso = WIN32OLE.new("Scripting.FileSystemObject")
    #  fso.openTextFile(filename, WIN32OLE::Variant::NoParam, false)
    NoParam = _
    # represents Nothing of VB.NET or VB.
    Nothing = _
    # represents VT_NULL OLE object.
    Null = _

    # Returns Ruby object wrapping OLE variant whose variant type is VT_ARRAY.
    # The first argument should be Array object which specifies dimensions
    # and each size of dimensions of OLE array.
    # The second argument specifies variant type of the element of OLE array.
    #
    # The following create 2 dimensions OLE array. The first dimensions size
    # is 3, and the second is 4.
    #
    #    ole_ary = WIN32OLE::Variant.array([3,4], VT_I4)
    #    ruby_ary = ole_ary.value # => [[0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0]]
    def self.array(ary, vt) end

    # Returns Ruby object wrapping OLE variant.
    # The first argument specifies Ruby object to convert OLE variant variable.
    # The second argument specifies VARIANT type.
    # In some situation, you need the WIN32OLE::Variant object to pass OLE method
    #
    #    shell = WIN32OLE.new("Shell.Application")
    #    folder = shell.NameSpace("C:\\Windows")
    #    item = folder.ParseName("tmp.txt")
    #    # You can't use Ruby String object to call FolderItem.InvokeVerb.
    #    # Instead, you have to use WIN32OLE::Variant object to call the method.
    #    shortcut = WIN32OLE::Variant.new("Create Shortcut(\&S)")
    #    item.invokeVerb(shortcut)
    def initialize(val, vartype) end

    # Returns the element of WIN32OLE::Variant object(OLE array).
    # This method is available only when the variant type of
    # WIN32OLE::Variant object is VT_ARRAY.
    #
    # REMARK:
    #    The all indices should be 0 or natural number and
    #    lower than or equal to max indices.
    #    (This point is different with Ruby Array indices.)
    #
    #    obj = WIN32OLE::Variant.new([[1,2,3],[4,5,6]])
    #    p obj[0,0] # => 1
    #    p obj[1,0] # => 4
    #    p obj[2,0] # => WIN32OLE::RuntimeError
    #    p obj[0, -1] # => WIN32OLE::RuntimeError
    def [](i, j, *args) end

    # Set the element of WIN32OLE::Variant object(OLE array) to val.
    # This method is available only when the variant type of
    # WIN32OLE::Variant object is VT_ARRAY.
    #
    # REMARK:
    #    The all indices should be 0 or natural number and
    #    lower than or equal to max indices.
    #    (This point is different with Ruby Array indices.)
    #
    #    obj = WIN32OLE::Variant.new([[1,2,3],[4,5,6]])
    #    obj[0,0] = 7
    #    obj[1,0] = 8
    #    p obj.value # => [[7,2,3], [8,5,6]]
    #    obj[2,0] = 9 # => WIN32OLE::RuntimeError
    #    obj[0, -1] = 9 # => WIN32OLE::RuntimeError
    def []=(i, j, *args, val) end

    # Returns Ruby object value from OLE variant.
    #    obj = WIN32OLE::Variant.new(1, WIN32OLE::VARIANT::VT_BSTR)
    #    obj.value # => "1" (not Integer object, but String object "1")
    def value; end

    # Sets variant value to val. If the val type does not match variant value
    # type(vartype), then val is changed to match variant value type(vartype)
    # before setting val.
    # This method is not available when vartype is VT_ARRAY(except VT_UI1|VT_ARRAY).
    # If the vartype is VT_UI1|VT_ARRAY, the val should be String object.
    #
    #    obj = WIN32OLE::Variant.new(1) # obj.vartype is WIN32OLE::VARIANT::VT_I4
    #    obj.value = 3.2 # 3.2 is changed to 3 when setting value.
    #    p obj.value # => 3
    def value=(val) end

    # Returns OLE variant type.
    #    obj = WIN32OLE::Variant.new("string")
    #    obj.vartype # => WIN32OLE::VARIANT::VT_BSTR
    def vartype; end
  end

  # The +WIN32OLE::VariantType+ module includes constants of VARIANT type constants.
  # The constants is used when creating WIN32OLE::Variant object.
  #
  #   obj = WIN32OLE::Variant.new("2e3", WIN32OLE::VARIANT::VT_R4)
  #   obj.value # => 2000.0
  module VariantType
    # represents VT_ARRAY type constant.
    VT_ARRAY = _
    # represents VT_BOOL type constant.
    VT_BOOL = _
    # represents VT_BSTR type constant.
    VT_BSTR = _
    # represents VT_BYREF type constant.
    VT_BYREF = _
    # represents VT_CY type constant.
    VT_CY = _
    # represents VT_DATE type constant.
    VT_DATE = _
    # represents VT_DISPATCH type constant.
    VT_DISPATCH = _
    # represents VT_EMPTY type constant.
    VT_EMPTY = _
    # represents VT_ERROR type constant.
    VT_ERROR = _
    # represents VT_I1 type constant.
    VT_I1 = _
    # represents VT_I2 type constant.
    VT_I2 = _
    # represents VT_I4 type constant.
    VT_I4 = _
    # represents VT_I8 type constant.
    VT_I8 = _
    # represents VT_INT type constant.
    VT_INT = _
    # represents VT_NULL type constant.
    VT_NULL = _
    # represents VT_PTR type constant.
    VT_PTR = _
    # represents VT_R4 type constant.
    VT_R4 = _
    # represents VT_R8 type constant.
    VT_R8 = _
    # represents VT_UI1 type constant.
    VT_UI1 = _
    # represents VT_UI2 type constant.
    VT_UI2 = _
    # represents VT_UI4 type constant.
    VT_UI4 = _
    # represents VT_UI8 type constant.
    VT_UI8 = _
    # represents VT_UINT type constant.
    VT_UINT = _
    # represents VT_UNKNOWN type constant.
    VT_UNKNOWN = _
    # represents VT_USERDEFINED type constant.
    VT_USERDEFINED = _
    # represents VT_VARIANT type constant.
    VT_VARIANT = _
  end
end
