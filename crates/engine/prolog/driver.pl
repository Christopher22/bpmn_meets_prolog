/*  driver.pl -- host driver: load a user program, render its model/1
    =================================================================

    The host (Rust) runs two queries per render.  First

        render(+Clauses, -Result)

    with the user program already split into clauses (by bpmn-syntax):

        Clauses     = [c(Line, Chars), ...]
        Result      = result(Outcome, Diagnostics)
        Outcome     = rendered(Problems) | no_model | failed(Message)
        Problems    = [problem(Code, [Arg, ...]), ...]   (from bpmn_check/2)
        Diagnostics = [diagnostic(Line, Message), ...]

    where Message and Arg are atoms.  Then, if the outcome was rendered,

        write_diagram

    writes the BPMN 2.0 XML to user_error.  Large char lists are not
    returned as answers because Scryer's answer conversion is quadratic in
    their length; the host captures the stream instead.

    Why not consult the program?  The system loader aborts on the first
    failing directive (e.g. `:- use_module(bpmn).`, which has no file to
    load in the browser) and leaves the machine unusable.  Here every clause
    is read and asserted on its own inside catch/3, so one bad clause yields
    a diagnostic and the rest of the program still loads.

    Both predicates are deterministic and never fail or throw.
*/

:- module(driver, [render/2, write_diagram/0]).

:- use_module(library(bpmn)).
:- use_module(library(charsio)).
:- use_module(library(dcgs)).
:- use_module(library(format)).
:- use_module(library(iso_ext)).
:- use_module(library(lists)).

%  owned(Name, Arity): user predicates created by the current program.
:- dynamic(owned/2).

render(Clauses, result(Outcome, Diagnostics)) :-
    bb_put(driver_diagram, []),
    unload,
    load(Clauses, Diagnostics),
    outcome(Outcome),
    catch(flush_output(user_output), _, true),
    catch(flush_output(user_error), _, true).

write_diagram :-
    bb_get(driver_diagram, Xml),
    bb_put(driver_diagram, []),
    catch(( format(user_error, "~s", [Xml]), flush_output(user_error) ), _, true).

/* ---------------------------------------------------------------------
   Loading
   --------------------------------------------------------------------- */

unload :-
    (   retract(owned(Name, Arity)),
        catch(abolish(user:Name/Arity), _, true),
        false
    ;   true
    ).

load([], []).
load([c(Line, Chars)|Cs], Ds0) :-
    catch(( read_term_from_chars(Chars, Term, []),
            expand(Term, Terms),
            load_terms(Terms, Line, Ds0, Ds) ),
          E,
          ( message(E, M), Ds0 = [diagnostic(Line, M)|Ds] )),
    load(Cs, Ds).

%  term_expansion/2 hooks in user (library(dcgs) installs one for -->).
expand(Term0, Terms) :-
    (   catch(user:term_expansion(Term0, Term), _, false)
    ->  (   Term = throw_dcg_expansion_error(E) -> throw(E)
        ;   Term = [_|_] -> Terms = Term
        ;   Term == [] -> Terms = []
        ;   Terms = [Term]
        )
    ;   Terms = [Term0]
    ).

load_terms([], _, Ds, Ds).
load_terms([T|Ts], Line, Ds0, Ds) :-
    load_term(T, Line, Ds0, Ds1),
    load_terms(Ts, Line, Ds1, Ds).

load_term(Term, _, _, _) :-
    var(Term), !,
    throw(error(instantiation_error, load/1)).
load_term((:- Directive), Line, Ds0, Ds) :- !,
    directive(Directive, Line, Ds0, Ds).
load_term(Clause, _, Ds, Ds) :-
    clause_head(Clause, Head),
    functor(Head, Name, Arity),
    assertz(user:Clause),
    (   owned(Name, Arity) -> true
    ;   assertz(owned(Name, Arity))
    ).

clause_head((Head :- _), Head) :- !.
clause_head(Head, Head).

%  Modules: bpmn and the system libraries are compiled in; there are no
%  files to load in the browser.
directive(use_module(M), Line, Ds0, Ds) :- !,
    directive(use_module(M, all), Line, Ds0, Ds).
directive(use_module(M0, Imports), Line, Ds0, Ds) :- !,
    (   library_module(M0, M)
    ->  (   Imports == all -> user:use_module(M) ; user:use_module(M, Imports) ),
        Ds0 = Ds
    ;   unsupported(Line, "only bpmn and library(_) modules are available: ~q", [M0], Ds0, Ds)
    ).
directive(initialization(G), Line, Ds0, Ds) :- !,
    unsupported(Line, "initialization/1 goals are not run here: ~q", [G], Ds0, Ds).
directive(initialization(G, _), Line, Ds0, Ds) :- !,
    directive(initialization(G), Line, Ds0, Ds).
directive(Goal, Line, Ds0, Ds) :-
    (   call(user:Goal) -> Ds0 = Ds
    ;   unsupported(Line, "directive failed: ~q", [Goal], Ds0, Ds)
    ).

library_module(bpmn, library(bpmn)).
library_module(library(L), library(L)).

unsupported(Line, Format, Args, [diagnostic(Line, M)|Ds], Ds) :-
    formatted(Format, Args, M).

/* ---------------------------------------------------------------------
   Rendering
   --------------------------------------------------------------------- */

outcome(Outcome) :-
    catch(outcome_(Outcome), E, ( message(E, M), Outcome = failed(M) )).

outcome_(Outcome) :-
    (   \+ owned(model, 1)
    ->  Outcome = no_model
    ;   once(user:model(Model))
    ->  model_outcome(Model, Outcome)
    ;   Outcome = failed('model/1 has no solution')
    ).

model_outcome(Model, Outcome) :-
    (   nonvar(Model), Model = bpmn(_, _)
    ->  bpmn_check(Model, Ps),
        maplist(problem, Ps, Problems),
        bpmn_chars(Model, Xml),
        bb_put(driver_diagram, Xml),
        Outcome = rendered(Problems)
    ;   formatted("model/1 must yield bpmn(Id, Items), not ~q", [Model], M),
        Outcome = failed(M)
    ).

problem(P, problem(Code, Args)) :-
    P =.. [Code|As],
    maplist(arg_atom, As, Args).

arg_atom(A, Atom) :-
    (   atom(A) -> Atom = A
    ;   formatted("~q", [A], Atom)
    ).

message(error(syntax_error(What), _), M) :- !,
    formatted("syntax error: ~w", [What], M).
message(error(Formal, _), M) :- !,
    formatted("~q", [Formal], M).
message(Ball, M) :-
    formatted("uncaught exception: ~q", [Ball], M).

formatted(Format, Args, Atom) :-
    phrase(format_(Format, Args), Cs),
    atom_chars(Atom, Cs).
