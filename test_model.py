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


if __name__ == '__main__': unittest.main()
