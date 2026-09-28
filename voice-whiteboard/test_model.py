import copy
import unittest
from model import fresh, operation, select, patch, evaluate, upgrade, BUILTIN_PRESETS, validate_preset


class ModelTests(unittest.TestCase):
    def setUp(self): self.s = fresh()
    def runop(self, **c):
        candidate = copy.deepcopy(self.s); selected = operation(candidate, c); self.s = candidate; return selected
    def dots(self): self.runop(op='create', object={'id': 'd', 'type': 'dot', 'color': '#0088ff'}, count=10)
    def card(self, i='c', **params): self.runop(op='create', object={'id': i, 'type': 'card', 'title': 'T', 'text': 'body', **params})

    # ---- presets
    def test_card_expands_into_primitives_with_stable_child_ids(self):
        self.card()
        self.assertEqual(self.s['objects']['c']['type'], 'group')
        self.assertEqual(self.s['objects']['c']['children'], ['c/bg', 'c/title', 'c/body'])
        self.assertEqual(self.s['objects']['c/body']['text'], 'body')
        self.assertEqual(self.s['objects']['c/bg']['w'], 'fill')

    def test_single_item_preset_is_an_alias_not_a_group(self):
        self.dots()
        self.assertEqual(self.s['objects']['d-0']['type'], 'ellipse')
        self.assertEqual(self.s['objects']['d-0']['preset'], 'dot')

    def test_parameter_change_reexpands_and_keeps_edges(self):
        self.card(); self.dots()
        self.runop(op='link', id='e', **{'from': 'c/body', 'to': 'd-0'})
        self.runop(op='set', select='c', props={'text': 'new'})
        self.assertEqual(self.s['objects']['c/body']['text'], 'new')
        self.assertIn('e', self.s['objects'])

    def test_parameter_change_keeps_instance_group_edits(self):
        self.card()
        self.runop(op='set', select='c', props={'outline': True, 'padding': 30})
        self.runop(op='set', select='c', props={'text': 'x'})
        self.assertTrue(self.s['objects']['c']['outline'])
        self.assertEqual(self.s['objects']['c']['padding'], 30)
        self.assertEqual(self.s['objects']['c/body']['text'], 'x')

    def test_parameter_change_keeps_removed_instance_group_field(self):
        self.card()
        self.runop(op='set', select='c', props={'layout': None})
        self.runop(op='set', select='c', props={'text': 'x'})
        self.assertNotIn('layout', self.s['objects']['c'])

    def test_redefined_preset_drops_obsolete_parameters_on_reexpand(self):
        def definition(params, parameter):
            return {'name': 'badge', 'params': params, 'items': [
                {'id': 'label', 'type': 'text', 'text': '${' + parameter + '}'}, {'id': 'back', 'type': 'rect', 'w': 20, 'h': 20}]}
        self.runop(op='define', preset=definition({'old': 'first'}, 'old'))
        self.runop(op='create', object={'id': 'badge-1', 'type': 'badge', 'old': 'custom'})
        self.runop(op='define', preset=definition({'new': 'second'}, 'new'))
        self.runop(op='set', select='badge-1', props={'new': 'updated'})
        self.assertEqual(self.s['objects']['badge-1']['params'], {'new': 'updated'})
        self.assertEqual(self.s['objects']['badge-1/label']['text'], 'updated')

    def test_redefined_alias_updates_its_primitive_fields(self):
        self.runop(op='define', preset={'name': 'tag', 'params': {'old': 'before'},
                                        'items': [{'id': 'text', 'type': 'text', 'text': '${old}'}]})
        self.runop(op='create', object={'id': 'tag-1', 'type': 'tag', 'old': 'mine'})
        self.runop(op='define', preset={'name': 'tag', 'params': {'new': 'after'},
                                        'items': [{'id': 'text', 'type': 'text', 'text': '${new}'}]})
        self.runop(op='set', select='tag-1', props={'new': 'changed'})
        self.assertEqual(self.s['objects']['tag-1']['params'], {'new': 'changed'})
        self.assertEqual(self.s['objects']['tag-1']['text'], 'changed')

    def test_redefined_alias_with_default_parameters_preserves_explicit_edits(self):
        self.runop(op='define', preset={'name': 'tag', 'params': {'old': 'before'},
                                        'items': [{'id': 'text', 'type': 'text', 'text': '${old}', 'size': 12, 'color': 'blue'}]})
        self.runop(op='create', object={'id': 'tag-1', 'type': 'tag'})
        self.runop(op='set', select='tag-1', props={'size': 30, 'color': 'red'})
        self.runop(op='define', preset={'name': 'tag', 'params': {'new': 'after'},
                                        'items': [{'id': 'text', 'type': 'text', 'text': '${new}', 'size': 12, 'color': 'blue'}]})
        self.runop(op='set', select='tag-1', props={'new': 'updated'})
        obj = self.s['objects']['tag-1']
        self.assertEqual((obj['text'], obj['size'], obj['color']), ('updated', 30, 'red'))
        self.assertNotIn('old', obj['params'])

    def test_ungroup_clears_redefinition_baseline_before_reusing_id(self):
        def definition(width):
            return {'name': 'tag', 'params': {'width': width}, 'group': {'w': {'$': 'width'}},
                    'items': [{'id': 'body', 'type': 'rect', 'w': 5, 'h': 5}]}
        self.runop(op='define', preset=definition(10))
        self.runop(op='create', object={'id': 'g', 'type': 'tag'})
        self.runop(op='define', preset=definition(20))
        self.runop(op='ungroup', select='g')
        self.runop(op='remove', select='g/body')
        self.runop(op='create', object={'id': 'g', 'type': 'tag'})
        self.runop(op='set', select='g', props={'width': 30})
        self.assertEqual(self.s['objects']['g']['w'], 30)
        self.assertNotIn('g', self.s.get('preset_baselines', {}))

    def test_direct_child_edit_protects_against_silent_loss(self):
        self.card()
        self.runop(op='set', select='c/body', props={'color': '#ff0000'})
        self.assertTrue(self.s['objects']['c']['overridden'])
        with self.assertRaises(ValueError): self.runop(op='set', select='c', props={'text': 'x'})
        self.runop(op='set', select='c', props={'text': 'x'}, resetOverrides=True)
        self.assertNotIn('overridden', self.s['objects']['c'])
        self.assertEqual(self.s['objects']['c/body']['color'], '#dde5f1')

    def test_unknown_parameter_names_the_accepted_ones(self):
        with self.assertRaisesRegex(ValueError, 'accepts .*title'): self.card(bogus=1)

    def test_optional_title_omitted_via_when(self):
        self.runop(op='create', object={'id': 'c', 'type': 'card', 'text': 'only body'})
        self.assertEqual(self.s['objects']['c']['children'], ['c/bg', 'c/body'])

    def test_user_preset_with_expressions_and_repeat(self):
        with self.assertRaises(ValueError):  # repeat over something that is not an array parameter fails at define time
            self.runop(op='define', preset={'name': 'stairs', 'params': {'n': 3, 'step': 20}, 'items': [
                {'repeat': 'steps', 'as': 's', 'index': 'i', 'items': [{'id': 'r${i}', 'type': 'rect', 'w': {'$': 'step*(i+1)'}, 'h': {'$': 'step'}, 'y': {'$': 'i*step'}}]}]})
        self.runop(op='define', preset={'name': 'stairs', 'params': {'steps': [1, 2, 3], 'step': 20}, 'items': [
            {'repeat': 'steps', 'as': 's', 'index': 'i', 'items': [{'id': 'r${i}', 'type': 'rect', 'w': {'$': 'step*s'}, 'h': {'$': 'step'}, 'y': {'$': 'i*step'}}]}]})
        self.runop(op='create', object={'id': 'x', 'type': 'stairs', 'steps': [1, 2]})
        self.assertEqual(self.s['objects']['x']['children'], ['x/r0', 'x/r1'])
        self.assertEqual(self.s['objects']['x/r1']['w'], 40)
        self.assertEqual(self.s['objects']['x/r1']['y'], 20)

    def test_literal_text_is_not_reinterpreted(self):
        self.card(text='costs ${price}')
        self.assertEqual(self.s['objects']['c/body']['text'], 'costs ${price}')

    def test_expression_language_is_tiny(self):
        self.assertEqual(evaluate('(w - 30) / 2', {'w': 100}), 35)
        self.assertEqual(evaluate('-size', {'size': 5}), -5)
        for bad in ('w ** 2', 'import os', '1/0', 'w +', '__x__'):
            with self.assertRaises(ValueError): evaluate(bad, {'w': 1})

    def test_builtin_presets_are_valid(self):
        for d in BUILTIN_PRESETS.values(): validate_preset(d)

    def test_table_nests_cells_under_the_instance(self):
        self.runop(op='create', object={'id': 't', 'type': 'table', 'rows': [['a', 'b'], ['c', 'd']], 'cols': 2})
        self.assertEqual(self.s['objects']['t/cell-1-0']['parent'], 't')
        self.assertEqual(self.s['objects']['t/cell-1-0/text']['parent'], 't/cell-1-0')
        self.assertEqual(self.s['objects']['t/cell-1-0/text']['text'], 'c')

    # ---- tree
    def test_group_converts_children_to_local_coordinates(self):
        self.dots(); self.runop(op='move', select='d-1', to=[100, 50]); self.runop(op='move', select='d-0', to=[20, 10])
        self.runop(op='group', id='g', select=['d-0', 'd-1'])
        g = self.s['objects']['g']
        self.assertEqual((g['x'], g['y']), (20, 10))
        self.assertEqual((self.s['objects']['d-1']['x'], self.s['objects']['d-1']['y']), (80, 40))
        self.runop(op='move', select='g', by=[5, 5])
        self.assertEqual(self.s['objects']['d-1']['x'], 80)  # children do not move in their own frame
        self.runop(op='ungroup', select='g')
        self.assertEqual((self.s['objects']['d-1']['x'], self.s['objects']['d-1']['y']), (105, 55))

    def test_move_group_and_descendant_translates_once(self):
        self.runop(op='create', items=[
            {'id': 'g', 'type': 'group', 'x': 100, 'y': 20, 'z': 10},
            {'id': 'child', 'type': 'rect', 'x': 125, 'y': 25, 'z': 13, 'w': 20, 'h': 20}])
        self.runop(op='reparent', select='child', into='g')
        self.runop(op='move', select={}, by=[7, 9, 4])
        objects = self.s['objects']
        self.assertEqual((objects['g']['x'], objects['g']['y'], objects['g']['z']), (107, 29, 14))
        self.assertEqual((objects['child']['x'], objects['child']['y'], objects['child']['z']), (25, 5, 3))


    def test_reparent_keeps_world_position(self):
        self.dots(); self.runop(op='create', object={'id': 'g', 'type': 'group', 'x': 100, 'y': 100})
        self.runop(op='move', select='d-0', to=[130, 140])
        self.runop(op='reparent', select='d-0', into='g')
        self.assertEqual((self.s['objects']['d-0']['x'], self.s['objects']['d-0']['y']), (30, 40))
        self.assertEqual(self.s['objects']['g']['children'], ['d-0'])
        self.runop(op='reparent', select='d-0', into=None)
        self.assertEqual((self.s['objects']['d-0']['x'], self.s['objects']['d-0']['y']), (130, 140))

    def test_moving_a_laid_out_child_is_rejected_with_guidance(self):
        self.card()
        with self.assertRaisesRegex(ValueError, 'positioned by the layout'): self.runop(op='move', select='c/body', by=[1, 1])

    def test_removing_a_group_removes_children_and_edges(self):
        self.card(); self.dots(); self.runop(op='link', id='e', **{'from': 'd-0', 'to': 'c/title'})
        self.runop(op='remove', select='c')
        self.assertFalse({'c', 'c/bg', 'c/title', 'e'} & set(self.s['objects']))

    # ---- edges
    def test_edge_anchors_validated(self):
        self.runop(op='create', object={'id': 'p', 'type': 'polygon', 'points': [[0, 0], [10, 0], [10, 10], [0, 10]]}); self.dots()
        self.runop(op='link', id='ok', **{'from': {'id': 'p', 'vertex': 3}, 'to': {'id': 'd-0', 'side': 'left', 'offset': 0.3}})
        self.runop(op='link', id='pt', **{'from': [0, 0], 'to': 'd-1'})
        with self.assertRaises(ValueError): self.runop(op='link', id='v', **{'from': {'id': 'p', 'vertex': 4}, 'to': 'd-0'})
        with self.assertRaises(ValueError): self.runop(op='link', id='s', **{'from': {'id': 'd-0', 'side': 'diagonal'}, 'to': 'd-1'})
        with self.assertRaises(ValueError): self.runop(op='link', id='ee', **{'from': 'ok', 'to': 'd-1'})
        self.runop(op='set', select='p', props={'points': [[0, 0], [1, 0], [0, 1], [1, 1], [2, 2]]})  # growing is fine
        with self.assertRaisesRegex(ValueError, 'no vertex 3'):  # shrinking below a referenced vertex is not
            self.runop(op='set', select='p', props={'points': [[0, 0], [1, 0], [1, 1]]})

    def test_fixed_3d_anchor_validates_each_coordinate(self):
        self.runop(op='link', id='point', **{'from': [0, 0], 'to': [10, 20, 30]})
        self.assertEqual(self.s['objects']['point']['to'], [10, 20, 30])
        for bad in ([1], [1, 2, 3, 4], [1, 2, '3'], [1, 2, float('nan')]):
            with self.subTest(bad=bad):
                with self.assertRaises(ValueError):
                    self.runop(op='link', id='bad', **{'from': [0, 0], 'to': bad})

    # ---- selection and misc
    def test_type_filter_matches_preset_name(self):
        self.card(); self.dots()
        self.assertEqual(select(self.s, {'type': 'card'}), ['c'])
        self.assertEqual(select(self.s, {'preset': 'dot', 'limit': 2}), ['d-0', 'd-1'])
        self.assertEqual(select(self.s, {'parent': 'c'}), ['c/bg', 'c/title', 'c/body'])
        self.assertEqual(len(select(self.s, {'roots': True})), 11)

    def test_set_null_deletes_and_read_only_fields_rejected(self):
        self.runop(op='create', object={'id': 'r', 'type': 'rect', 'w': 10, 'h': 10, 'rx': 3})
        self.runop(op='set', select='r', props={'rx': None}); self.assertNotIn('rx', self.s['objects']['r'])
        for props in ({'box': {}}, {'parent': 'x'}, {'type': 'ellipse'}):
            with self.assertRaises(ValueError): self.runop(op='set', select='r', props=props)

    def test_sizing_keywords_validated(self):
        self.runop(op='create', object={'id': 'r', 'type': 'rect', 'w': 'fill', 'h': 10})
        with self.assertRaises(ValueError): self.runop(op='create', object={'id': 'r2', 'type': 'rect', 'w': 'hug', 'h': 10})
        with self.assertRaises(ValueError): self.runop(op='create', object={'id': 'r3', 'type': 'rect', 'w': -1, 'h': 10})
        self.runop(op='create', object={'id': 'g', 'type': 'group', 'w': 'hug', 'h': 'fill'})

    def test_view_requires_a_target(self):
        self.dots()
        with self.assertRaises(ValueError): self.runop(op='view')
        self.assertEqual(self.runop(op='view', fit={'type': 'dot', 'limit': 2}), ['d-0', 'd-1'])

    def test_patch_marks_only_changed_fields(self):
        self.dots(); old = copy.deepcopy(self.s); self.runop(op='set', select='d-0', props={'tags': ['x']})
        self.assertEqual(patch(old, self.s)['fields']['d-0'], ['tags'])

    def test_upgrade_from_v2_preserves_ids_and_relinks(self):
        old = {'objects': {
            'a': {'id': 'a', 'type': 'card', 'x': 0, 'y': 0, 'title': 'A', 'text': 'x', 'width': 300},
            'b': {'id': 'b', 'type': 'dot', 'x': 500, 'y': 0, 'radius': 6, 'color': '#ff0000'},
            'ab': {'id': 'ab', 'type': 'arrow', 'from': 'a', 'to': 'b', 'text': 'then'},
            'g': {'id': 'g', 'type': 'group', 'x': 100, 'y': 100, 'members': ['b'], 'title': 'box'},
        }, 'physics': {'enabled': True}}
        new = upgrade(old)
        self.assertEqual(new['objects']['a']['preset'], 'card'); self.assertEqual(new['objects']['a']['w'], 300)
        self.assertEqual(new['objects']['ab']['type'], 'edge'); self.assertEqual(new['objects']['ab']['label'], 'then'); self.assertEqual(new['objects']['ab']['head'], 'arrow')
        self.assertEqual(new['objects']['b']['parent'], 'g'); self.assertEqual(new['objects']['b']['x'], 400)
        self.assertTrue(new['physics']['enabled'])

    def test_upgrade_nested_groups_ignore_key_order_and_keep_edges_as_roots(self):
        old = {'objects': {
            'A': {'id': 'A', 'type': 'group', 'x': 10, 'y': 20, 'members': ['B', 'L']},
            'B': {'id': 'B', 'type': 'group', 'x': 110, 'y': 20, 'members': ['C']},
            'C': {'id': 'C', 'type': 'rectangle', 'x': 110, 'y': 70},
            'L': {'id': 'L', 'type': 'line', 'from': 'B', 'to': 'C'},
        }}
        new = upgrade(old)['objects']
        self.assertEqual((new['C']['x'] + new['B']['x'] + new['A']['x'], new['C']['y'] + new['B']['y'] + new['A']['y']), (110, 70))
        self.assertNotIn('parent', new['L']); self.assertEqual(new['A']['children'], ['B'])

    # ---- adversarial review regressions
    def test_nested_preset_child_edit_is_protected(self):
        self.runop(op='create', object={'id': 't', 'type': 'table', 'rows': [['a', 'b'], ['c', 'd']], 'cols': 2})
        cell = next(i for i, o in self.s['objects'].items() if o['type'] == 'text' and o['parent'] != 't' and o.get('text') == 'a')
        self.runop(op='set', select=cell, props={'text': 'FIXED'})
        with self.assertRaisesRegex(ValueError, 'resetOverrides'): self.runop(op='set', select='t', props={'cols': 1})

    def test_moving_a_preset_child_is_protected(self):
        self.runop(op='create', object={'id': 'k', 'type': 'cube', 'size': 50}); self.runop(op='move', select='k/back', by=[5, 0])
        with self.assertRaisesRegex(ValueError, 'resetOverrides'): self.runop(op='set', select='k', props={'size': 60})

    def test_impulse_applies_once_to_selected_descendants_of_one_root(self):
        self.runop(op='create', object={'id': 'k', 'type': 'cube', 'size': 50})
        self.runop(op='impulse', select={'parent': 'k'}, velocity=[10, 0])
        self.assertEqual(self.s['objects']['k']['vx'], 10)

    def test_group_props_cannot_impersonate_a_preset(self):
        self.runop(op='create', items=[{'id': 'p', 'type': 'rect', 'w': 5, 'h': 5}, {'id': 'q', 'type': 'rect', 'w': 5, 'h': 5}])
        with self.assertRaisesRegex(ValueError, 'preset'): self.runop(op='group', id='g', select=['p', 'q'], props={'preset': 'card'})

    def test_group_origin_from_props_keeps_member_world_positions(self):
        self.runop(op='create', items=[{'id': 'p', 'type': 'rect', 'x': 100, 'y': 0, 'z': 15, 'w': 5, 'h': 5}, {'id': 'q', 'type': 'rect', 'x': 200, 'y': 0, 'w': 5, 'h': 5}])
        self.runop(op='group', id='g', select=['p', 'q'], props={'x': 0, 'z': 100})
        o = self.s['objects']
        self.assertEqual((o['g']['x'] + o['p']['x'], o['g']['z'] + o['p']['z']), (100, 15))
        self.runop(op='ungroup', select='g')
        p = self.s['objects']['p']
        self.assertEqual((p['x'], p.get('z', 0)), (100, 15))

    def test_reparent_keeps_world_z(self):
        self.runop(op='create', items=[{'id': 'g', 'type': 'group', 'z': 50}, {'id': 'r', 'type': 'rect', 'z': 12, 'w': 5, 'h': 5}])
        self.runop(op='reparent', select='r', into='g')
        self.assertEqual(self.s['objects']['r']['z'], -38)
        self.runop(op='reparent', select='r', into=None)
        self.assertEqual(self.s['objects']['r']['z'], 12)

    def test_reparent_out_of_layout_uses_reported_origin(self):
        self.runop(op='create', items=[{'id': 'g', 'type': 'group', 'x': 100, 'y': 200, 'layout': {'type': 'stack'}}, {'id': 'r', 'type': 'rect', 'x': 999, 'y': 999, 'w': 5, 'h': 5}])
        self.runop(op='reparent', select='r', into='g')
        self.s['objects']['r']['box'] = {'x': 100, 'y': 200, 'w': 5, 'h': 5, 'ox': 100, 'oy': 200}
        self.runop(op='reparent', select='r', into=None)
        self.assertEqual((self.s['objects']['r']['x'], self.s['objects']['r']['y']), (100, 200))

    def test_builtin_presets_cannot_be_redefined(self):
        with self.assertRaisesRegex(ValueError, 'built-in'):
            self.runop(op='define', preset={'name': 'card', 'params': {}, 'items': [{'id': 'a', 'type': 'rect', 'w': 1, 'h': 1}]})

    def test_reexpand_refuses_to_overwrite_unrelated_object(self):
        self.runop(op='define', preset={'name': 'clash', 'params': {'items': ['a']}, 'items': [{'repeat': 'items', 'items': [{'id': '${item}', 'type': 'rect', 'w': 1, 'h': 1}]}]})
        self.runop(op='create', items=[{'id': 'inst/x', 'type': 'ellipse', 'r': 9}, {'id': 'inst', 'type': 'clash'}])
        with self.assertRaisesRegex(ValueError, 'unrelated'): self.runop(op='set', select='inst', props={'items': ['a', 'x']})

    def test_expansion_size_is_capped_before_materializing(self):
        big = list(range(500))
        d = {'name': 'cube3', 'params': {'n': big}, 'items': [{'repeat': 'n', 'items': [{'repeat': 'n', 'as': 'j', 'index': 'k', 'items': [{'id': 'c${item}-${j}', 'type': 'rect', 'w': 1, 'h': 1}]}]}]}
        with self.assertRaisesRegex(ValueError, '2000'): self.runop(op='define', preset=d)

    def test_preset_group_cannot_replace_children(self):
        with self.assertRaisesRegex(ValueError, 'children'):
            self.runop(op='define', preset={'name': 'bad', 'params': {}, 'items': [{'id': 'a', 'type': 'rect', 'w': 1, 'h': 1}], 'group': {'children': 'x'}})

    def test_paint_accepts_colours_not_urls(self):
        self.runop(op='create', object={'id': 'r', 'type': 'rect', 'w': 1, 'h': 1, 'fill': 'none', 'color': 'rgba(10, 20, 30, .5)'})
        with self.assertRaisesRegex(ValueError, 'colour'): self.runop(op='set', select='r', props={'fill': 'url(https://example.invalid/x.svg)'})

    def test_malformed_commands_raise_value_errors(self):
        for c in ({'op': 'create', 'object': 'x', 'count': 2}, {'op': 'physics', 'props': ['enabled']}, {'op': 'layout', 'select': {}, 'mode': 'spiral'},
                  {'op': 'create', 'object': {'id': 'a', 'type': 'rect', 'w': 1, 'h': 1}, 'duration': float('nan')}):
            with self.assertRaises(ValueError): self.runop(**c)

    def test_reported_layout_origin_follows_a_moved_parent(self):
        self.runop(op='create', items=[{'id': 'L', 'type': 'group', 'layout': {'type': 'stack'}}, {'id': 'r', 'type': 'rect', 'w': 5, 'h': 5}])
        self.runop(op='reparent', select='r', into='L')
        self.s['objects']['L']['box'] = {'x': 0, 'y': 0, 'w': 5, 'h': 5, 'ox': 0, 'oy': 0}
        self.s['objects']['r']['box'] = {'x': 8, 'y': 8, 'w': 5, 'h': 5, 'ox': 8, 'oy': 8}
        self.runop(op='move', select='L', by=[100, 0])
        self.runop(op='reparent', select='r', into=None)
        self.assertEqual((self.s['objects']['r']['x'], self.s['objects']['r']['y']), (108, 8))

    def test_ungroup_inside_a_layout_is_refused(self):
        self.runop(op='create', items=[{'id': 'L', 'type': 'group', 'layout': {'type': 'stack'}}, {'id': 'G', 'type': 'group'}])
        self.runop(op='reparent', select='G', into='L')
        with self.assertRaisesRegex(ValueError, 'laid-out'): self.runop(op='ungroup', select='G')

    def test_reexpansion_keeps_reported_boxes(self):
        self.card(); self.s['objects']['c/body']['box'] = {'x': 1, 'y': 2, 'w': 3, 'h': 4, 'ox': 1, 'oy': 2}
        self.runop(op='set', select='c', props={'text': 'longer'})
        self.assertEqual(self.s['objects']['c/body']['box']['ox'], 1)

    def test_structural_edits_inside_a_preset_are_protected(self):
        self.runop(op='create', object={'id': 'k', 'type': 'cube', 'size': 50})
        self.runop(op='group', id='pair', select=self.s['objects']['k']['children'][:2])
        with self.assertRaisesRegex(ValueError, 'resetOverrides'): self.runop(op='set', select='k', props={'size': 60})
        self.runop(op='create', items=[{'id': 't', 'type': 'table', 'rows': [['a', 'b']], 'cols': 2}, {'id': 'extra', 'type': 'rect', 'w': 1, 'h': 1}])
        cell = next(i for i in self.s['objects']['t']['children'] if self.s['objects'][i]['type'] == 'group')
        self.runop(op='reparent', select='extra', into=cell)
        with self.assertRaisesRegex(ValueError, 'resetOverrides'): self.runop(op='set', select='t', props={'cols': 1})


if __name__ == '__main__': unittest.main()
