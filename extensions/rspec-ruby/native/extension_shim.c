#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include <mruby.h>
#include <mruby/compile.h>
#include <mruby/irep.h>
#include <mruby/string.h>
#include <mruby/variable.h>

extern const uint8_t rspec_ruby_mrb[];
extern const uint32_t rspec_ruby_mrb_len;

#ifdef __cplusplus
extern "C" {
#endif

static mrb_state *state = NULL;
static uint8_t *last_output = NULL;
static int32_t last_output_len = 0;

static int64_t pack_ptr_len(const uint8_t *ptr, int32_t len) {
  return (((int64_t)(uintptr_t)ptr) << 32) | (uint32_t)len;
}

static void ensure_state(void) {
  if (state != NULL) {
    return;
  }

  state = mrb_open();
  if (state == NULL) {
    abort();
  }

  mrb_load_irep(state, rspec_ruby_mrb);
  if (state->exc) {
    abort();
  }
}

static int64_t call_json_method(const char *method_name, const uint8_t *input, int32_t input_len) {
  ensure_state();

  if (last_output != NULL) {
    free(last_output);
    last_output = NULL;
    last_output_len = 0;
  }

  mrb_value result;
  mrb_value entrypoint = mrb_obj_value(mrb_module_get(state, "RubyFastLspExtensionEntrypoint"));
  if (input == NULL) {
    result = mrb_funcall(state, entrypoint, method_name, 0);
  } else {
    mrb_value input_string = mrb_str_new(state, (const char *)input, input_len);
    result = mrb_funcall(state, entrypoint, method_name, 1, input_string);
  }

  if (state->exc) {
    abort();
  }

  const char *output = RSTRING_PTR(result);
  last_output_len = (int32_t)RSTRING_LEN(result);
  last_output = (uint8_t *)malloc((size_t)last_output_len);
  if (last_output == NULL && last_output_len > 0) {
    abort();
  }
  memcpy(last_output, output, (size_t)last_output_len);

  return pack_ptr_len(last_output, last_output_len);
}

uint8_t *alloc(int32_t len) {
  if (len < 0) {
    abort();
  }
  return (uint8_t *)malloc((size_t)len);
}

void dealloc(uint8_t *ptr, int32_t len) {
  (void)len;
  free(ptr);
}

int32_t abi_version(void) {
  return 1;
}

int64_t indexed_call_names(void) {
  return call_json_method("indexed_call_names_json", NULL, 0);
}

int64_t index_call(uint8_t *ptr, int32_t len) {
  return call_json_method("index_call_json", ptr, len);
}

int64_t handle_event(uint8_t *ptr, int32_t len) {
  return call_json_method("handle_event_json", ptr, len);
}

#ifdef __cplusplus
}
#endif
