Evidence type: gameplay transcript

# parish-input Test Transcript

## Commands Run

```sh
cargo fmt -p parish-input
cargo clippy -p parish-input
cargo test -p parish-input
```

## Results

```
   Compiling parish-types v0.1.0
   Compiling parish-config v0.1.0
   Compiling parish-inference v0.1.0
   Compiling parish-input v0.1.0
    Finished test profile [unoptimized + debuginfo] target(s) in 2.79s
     Running unittests src/lib.rs

running 139 tests
test commands::tests::test_validate_branch_name_at_max_length ... ok
test commands::tests::test_validate_branch_name_invalid_chars ... ok
test commands::tests::test_validate_branch_name_just_over_max ... ok
test commands::tests::test_validate_branch_name_too_long ... ok
test commands::tests::test_validate_branch_name_valid ... ok
test commands::tests::test_validate_branch_name_with_special_chars ... ok
test commands::tests::test_validate_flag_name_at_max_length ... ok
test commands::tests::test_validate_flag_name_empty ... ok
test commands::tests::test_validate_flag_name_invalid_chars ... ok
test commands::tests::test_validate_flag_name_too_long ... ok
test commands::tests::test_validate_flag_name_valid ... ok
test intent_llm::tests::test_intent_response_deserialize ... ok
test intent_llm::tests::test_intent_response_empty ... ok
test intent_local::tests::test_local_parse_amble ... ok
test intent_local::tests::test_local_parse_bare_unusual_verbs_no_target ... ok
test intent_local::tests::test_local_parse_case_insensitive ... ok
test intent_local::tests::test_local_parse_creep_sneak_bolt_scramble ... ok
test intent_local::tests::test_local_parse_empty_target ... ok
test intent_local::tests::test_local_parse_first_person_narrative_is_talk ... ok
test intent_local::tests::test_local_parse_go_shorthand ... ok
test intent_local::tests::test_local_parse_go_to ... ok
test intent_local::tests::test_local_parse_head_to ... ok
test intent_local::tests::test_local_parse_hurry_rush ... ok
test intent_local::tests::test_local_parse_look ... ok
test intent_local::tests::test_local_parse_meander_trot_stride ... ok
test intent_local::tests::test_local_parse_mosey ... ok
test intent_local::tests::test_local_parse_move_bare ... ok
test intent_local::tests::test_local_parse_multi_word_phrases ... ok
test intent_local::tests::test_local_parse_no_match ... ok
test intent_local::tests::test_local_parse_proceed ... ok
test intent_local::tests::test_local_parse_run_jog_dash ... ok
test intent_local::tests::test_local_parse_saunter ... ok
test intent_local::tests::test_local_parse_sprint_march_traipse ... ok
test intent_local::tests::test_local_parse_stroll ... ok
test intent_local::tests::test_local_parse_trek_and_hike ... ok
test intent_local::tests::test_local_parse_unusual_verbs_case_insensitive ... ok
test intent_local::tests::test_local_parse_visit ... ok
test intent_local::tests::test_local_parse_walk_to ... ok
test intent_local::tests::test_local_parse_wander ... ok
test intent_types::tests::test_intent_kind_deserialize ... ok
test mention::tests::test_extract_mention_at_mid_input ... ok
test mention::tests::test_extract_mention_at_not_after_space ... ok
test mention::tests::test_extract_mention_at_space ... ok
test mention::tests::test_extract_mention_bare_at ... ok
test mention::tests::test_extract_mention_connector_words ... ok
test mention::tests::test_extract_mention_first_word_must_be_uppercase ... ok
test mention::tests::test_extract_mention_full_name ... ok
test mention::tests::test_extract_mention_mid_with_rest ... ok
test mention::tests::test_extract_mention_name_only ... ok
test mention::tests::test_extract_mention_no_at ... ok
test mention::tests::test_extract_mention_simple_name ... ok
test mention::tests::test_extract_mention_trailing_punctuation ... ok
test mention::tests::test_extract_mention_trailing_punctuation_multiword ... ok
test mention::tests::test_extract_mention_whitespace_trimmed ... ok
test mention::tests::test_extract_mention_with_sentence ... ok
test parser::tests::test_classify_game_input ... ok
test parser::tests::test_classify_improv_command ... ok
test parser::tests::test_classify_irish_command ... ok
test parser::tests::test_classify_map_command ... ok
test parser::tests::test_classify_system_command ... ok
test parser::tests::test_classify_unknown_slash_command ... ok
test parser::tests::test_classify_whitespace ... ok
test parser::tests::test_fork_with_invalid_branch_name ... ok
test parser::tests::test_load_with_invalid_branch_name ... ok
test parser::tests::test_parse_about_command ... ok
test parser::tests::test_parse_about_command_case_insensitive ... ok
test parser::tests::test_parse_all_commands ... ok
test parser::tests::test_parse_category_all_show_and_set ... ok
test parser::tests::test_parse_category_invalid_category_returns_none ... ok
test parser::tests::test_parse_cloud_key_empty_name ... ok
test parser::tests::test_parse_cloud_key_set ... ok
test parser::tests::test_parse_cloud_key_show ... ok
test parser::tests::test_parse_cloud_model_empty_name ... ok
test parser::tests::test_parse_cloud_model_set ... ok
test parser::tests::test_parse_cloud_model_show ... ok
test parser::tests::test_parse_cloud_provider_empty_name ... ok
test parser::tests::test_parse_cloud_provider_set ... ok
test parser::tests::test_parse_cloud_provider_show_bare ... ok
test parser::tests::test_parse_cloud_show ... ok
test parser::tests::test_parse_cloud_unknown_subcommand ... ok
test parser::tests::test_parse_debug_bare ... ok
test parser::tests::test_parse_debug_case_insensitive ... ok
test parser::tests::test_parse_debug_with_empty_trailing_space ... ok
test parser::tests::test_parse_debug_with_subcommand ... ok
test parser::tests::test_parse_designer_command ... ok
test parser::tests::test_parse_flag_bare_shows_list ... ok
test parser::tests::test_parse_flag_enable ... ok
test parser::tests::test_parse_flag_enable_bare_shows_list ... ok
test parser::tests::test_parse_flag_invalid_name ... ok
test parser::tests::test_parse_flag_invalid_subcommand ... ok
test parser::tests::test_parse_flag_list ... ok
test parser::tests::test_parse_flags_alias ... ok
test parser::tests::test_parse_fork ... ok
test parser::tests::test_parse_fork_empty_name ... ok
test parser::tests::test_parse_improv_command ... ok
test parser::tests::test_parse_improv_command_case_insensitive ... ok
test parser::tests::test_parse_irish_command ... ok
test parser::tests::test_parse_irish_command_case_insensitive ... ok
test parser::tests::test_parse_key_set ... ok
test parser::tests::test_parse_key_show ... ok
test parser::tests::test_parse_load ... ok
test parser::tests::test_parse_load_empty_shows_picker ... ok
test parser::tests::test_parse_map_command ... ok
test parser::tests::test_parse_map_command_case_insensitive ... ok
test parser::tests::test_parse_model_set ... ok
test parser::tests::test_parse_model_show ... ok
test parser::tests::test_parse_new_command ... ok
test parser::tests::test_parse_npcs_command ... ok
test parser::tests::test_parse_preset_apply ... ok
test parser::tests::test_parse_preset_case_insensitive ... ok
test parser::tests::test_parse_preset_show_bare ... ok
test parser::tests::test_parse_provider_case_insensitive ... ok
test parser::tests::test_parse_provider_set ... ok
test parser::tests::test_parse_provider_show ... ok
test parser::tests::test_parse_quit ... ok
test parser::tests::test_parse_session_aliases ... ok
test parser::tests::test_parse_session_case_insensitive ... ok
test parser::tests::test_parse_speed_case_insensitive ... ok
test parser::tests::test_parse_speed_invalid_shows_error ... ok
test parser::tests::test_parse_speed_ludicrous ... ok
test parser::tests::test_parse_speed_set_variants ... ok
test parser::tests::test_parse_speed_show ... ok
test parser::tests::test_parse_speed_whitespace_shows_current ... ok
test parser::tests::test_parse_spinner_bare ... ok
test parser::tests::test_parse_spinner_clamped_to_max ... ok
test parser::tests::test_parse_spinner_invalid_duration ... ok
test parser::tests::test_parse_spinner_with_duration ... ok
test parser::tests::test_parse_theme_command ... ok
test parser::tests::test_parse_tick_command ... ok
test parser::tests::test_parse_time_command ... ok
test parser::tests::test_parse_unexplored_command ... ok
test parser::tests::test_parse_unknown_command ... ok
test parser::tests::test_parse_wait_command ... ok
test parser::tests::test_parse_wait_large_input_fallback ... ok
test parser::tests::test_parse_weather_bare ... ok
test parser::tests::test_parse_weather_case_insensitive ... ok
test parser::tests::test_parse_weather_set ... ok
test parser::tests::test_parse_where_command ... ok
test parser::tests::test_zero_arg_commands_reject_trailing_text ... ok

test result: ok. 139 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/llm_fallback_integration.rs

running 6 tests
test local_parse_bypasses_llm ... ok
test llm_fallback_examine_intent ... ok
test llm_fallback_http_error_returns_unknown ... ok
test llm_fallback_malformed_json_returns_unknown ... ok
test llm_fallback_missing_intent_field_defaults_to_unknown ... ok
test llm_fallback_success_returns_parsed_intent ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

   Doc-tests parish_input

running 1 test
test crates/parish-input/src/mention.rs - mention::MAX_MENTION_NAME_WORDS (line 35) ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```
